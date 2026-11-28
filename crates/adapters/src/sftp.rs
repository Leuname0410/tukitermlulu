//! Sistema de archivos remoto sobre el subsistema SFTP de la conexión SSH compartida.

use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use russh_sftp::client::RawSftpSession;
use russh_sftp::client::error::Error as SftpClientError;
use russh_sftp::protocol::{FileAttributes, OpenFlags, SftpHandle, StatusCode};
use tokio::io::{AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::Mutex;
use tuki_core::port_errors::{FsError, TransferError};
use tuki_core::ports::{
    CancelToken, ConnectionId, FileEntry, FileStat, FileTransfer, RemoteFileSystem,
    TransferProgress,
};
use tuki_core::remote_path::RemotePath;

use crate::ssh_transport::RusshTransport;

/// Ventana máxima de peticiones SFTP en vuelo por transferencia.
const TRANSFER_WINDOW: usize = 64;
/// Tamaño objetivo por petición cuando el servidor no impone un límite menor.
const TRANSFER_CHUNK: usize = 32 * 1024;

/// Etiqueta del resultado para logs: `ok` o la variante del error.
fn result_label<T, E: std::fmt::Debug>(result: &Result<T, E>) -> String {
    match result {
        Ok(_) => "ok".to_owned(),
        Err(error) => format!("{error:?}"),
    }
}

#[derive(Clone)]
struct CachedSession {
    session: Arc<RawSftpSession>,
    read_len: u32,
    write_len: u32,
}

/// Adaptador SFTP que reutiliza un canal por `ConnectionId`, sin nueva autenticación.
#[derive(Clone)]
pub struct RusshFileSystem {
    transport: RusshTransport,
    sessions: Arc<Mutex<HashMap<ConnectionId, CachedSession>>>,
}

impl RusshFileSystem {
    /// Crea el adaptador a partir del transporte con las conexiones ya autenticadas.
    pub fn new(transport: RusshTransport) -> Self {
        Self {
            transport,
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Devuelve la sesión cacheada o abre un subsistema `sftp` para la conexión.
    async fn cached(&self, conn: ConnectionId) -> Result<CachedSession, FsError> {
        if let Some(cached) = self.sessions.lock().await.get(&conn) {
            return Ok(cached.clone());
        }
        let channel = self
            .transport
            .open_subsystem(conn, "sftp")
            .await
            .map_err(|_| FsError::Operation)?;
        let mut session = RawSftpSession::new(channel.into_stream());
        session.init().await.map_err(map_sftp_error)?;
        let mut read_len = TRANSFER_CHUNK as u32;
        let mut write_len = TRANSFER_CHUNK as u32;
        if let Ok(limits) = session.limits().await {
            if limits.max_read_len > 0 {
                read_len = read_len.min(limits.max_read_len.min(u32::MAX as u64) as u32);
            }
            if limits.max_write_len > 0 {
                write_len = write_len.min(limits.max_write_len.min(u32::MAX as u64) as u32);
            }
            session.set_limits(limits.into());
        }
        let session = Arc::new(session);
        let cached = CachedSession {
            session,
            read_len: read_len.max(1),
            write_len: write_len.max(1),
        };
        let mut sessions = self.sessions.lock().await;
        Ok(sessions.entry(conn).or_insert(cached).clone())
    }

    async fn session(&self, conn: ConnectionId) -> Result<Arc<RawSftpSession>, FsError> {
        Ok(self.cached(conn).await?.session)
    }

    /// Descarta la sesión cacheada si el canal ya no sirve, permitiendo reabrirla.
    async fn invalidate(&self, conn: ConnectionId, session: &Arc<RawSftpSession>) {
        let mut sessions = self.sessions.lock().await;
        if sessions
            .get(&conn)
            .is_some_and(|cached| Arc::ptr_eq(&cached.session, session))
        {
            sessions.remove(&conn);
        }
    }

    /// Ejecuta una operación SFTP y limpia la caché si el canal se cerró.
    async fn run<T, F, Fut>(
        &self,
        conn: ConnectionId,
        operation: &'static str,
        path: Option<&str>,
        action: F,
    ) -> Result<T, FsError>
    where
        F: FnOnce(Arc<RawSftpSession>) -> Fut,
        Fut: Future<Output = Result<T, SftpClientError>> + Send,
    {
        let session = self.session(conn).await?;
        let result = match action(Arc::clone(&session)).await {
            Ok(value) => Ok(value),
            Err(error) => {
                if channel_closed(&error) {
                    self.invalidate(conn, &session).await;
                }
                Err(map_sftp_error(error))
            }
        };
        tracing::debug!(
            event = "sftp",
            operation,
            connection = conn.get(),
            path = path.unwrap_or(""),
            result = result_label(&result),
        );
        result
    }
}

impl RemoteFileSystem for RusshFileSystem {
    async fn list(&self, conn: ConnectionId, path: &RemotePath) -> Result<Vec<FileEntry>, FsError> {
        let entries = self
            .run(conn, "list", Some(path.as_str()), |session| async move {
                read_dir(&session, path.as_str()).await
            })
            .await?;
        Ok(entries
            .into_iter()
            .filter_map(|(name, metadata)| {
                if name == "." || name == ".." {
                    return None;
                }
                Some(FileEntry {
                    path: path.join(&name).ok()?,
                    is_dir: file_type_is_dir(&metadata),
                    size: metadata.size.unwrap_or(0),
                    permissions: metadata.permissions.map(|mode| mode & 0o7777),
                })
            })
            .collect())
    }

    async fn stat(&self, conn: ConnectionId, path: &RemotePath) -> Result<FileStat, FsError> {
        let metadata = self
            .run(conn, "stat", Some(path.as_str()), |session| async move {
                session.stat(path.as_str()).await.map(|attrs| attrs.attrs)
            })
            .await?;
        Ok(FileStat {
            size: metadata.size.unwrap_or(0),
            is_dir: file_type_is_dir(&metadata),
            permissions: metadata.permissions.map(|mode| mode & 0o7777),
        })
    }

    async fn rename(
        &self,
        conn: ConnectionId,
        from: &RemotePath,
        to: &RemotePath,
    ) -> Result<(), FsError> {
        self.run(conn, "rename", Some(from.as_str()), |session| async move {
            session.rename(from.as_str(), to.as_str()).await.map(|_| ())
        })
        .await
    }

    async fn remove(&self, conn: ConnectionId, path: &RemotePath) -> Result<(), FsError> {
        let stat = self.stat(conn, path).await?;
        self.run(conn, "remove", Some(path.as_str()), |session| async move {
            if stat.is_dir {
                session.rmdir(path.as_str()).await.map(|_| ())
            } else {
                session.remove(path.as_str()).await.map(|_| ())
            }
        })
        .await
    }

    async fn mkdir(&self, conn: ConnectionId, path: &RemotePath) -> Result<(), FsError> {
        self.run(conn, "mkdir", Some(path.as_str()), |session| async move {
            session
                .mkdir(path.as_str(), FileAttributes::default())
                .await
                .map(|_| ())
        })
        .await
    }

    async fn chmod(&self, conn: ConnectionId, path: &RemotePath, mode: u32) -> Result<(), FsError> {
        // Solo se envían los bits de permiso: el tipo de archivo no se toca.
        let metadata = FileAttributes {
            permissions: Some(mode & 0o7777),
            ..FileAttributes::default()
        };
        self.run(conn, "chmod", Some(path.as_str()), |session| async move {
            session.setstat(path.as_str(), metadata).await.map(|_| ())
        })
        .await
    }

    async fn home(&self, conn: ConnectionId) -> Result<RemotePath, FsError> {
        let home = self
            .run(conn, "home", None, |session| async move {
                session.realpath(".").await
            })
            .await?;
        let name = home
            .files
            .first()
            .ok_or(FsError::Operation)?
            .filename
            .to_string();
        RemotePath::new(&name).map_err(|_| FsError::Operation)
    }
}

impl FileTransfer for RusshFileSystem {
    async fn download(
        &self,
        conn: ConnectionId,
        remote: &RemotePath,
        local: &Path,
        progress: &dyn TransferProgress,
        cancel: &CancelToken,
    ) -> Result<u64, TransferError> {
        match tokio::fs::metadata(local).await {
            Ok(_) => return Err(TransferError::AlreadyExists),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(TransferError::LocalIo),
        }
        let stat = self.stat(conn, remote).await.map_err(map_fs_error)?;
        if stat.is_dir {
            return Err(TransferError::Remote);
        }
        let part = local_part(local);
        let cached = self.cached(conn).await.map_err(map_fs_error)?;
        let handle = open_handle(&cached.session, remote.as_str(), OpenFlags::READ).await?;
        // Creación exclusiva: un `.part` ajeno se respeta y no entra en la limpieza.
        let mut target = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&part)
            .await
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::AlreadyExists {
                    TransferError::AlreadyExists
                } else {
                    TransferError::LocalIo
                }
            })?;

        let result: Result<u64, TransferError> = async {
            let transferred = copy_download(
                &cached.session,
                &handle,
                &mut target,
                stat.size,
                cached.read_len,
                progress,
                cancel,
            )
            .await?;
            target.flush().await.map_err(|_| TransferError::LocalIo)?;
            drop(target);
            close_handle(&cached.session, handle.clone()).await?;
            tokio::fs::rename(&part, local)
                .await
                .map_err(|_| TransferError::LocalIo)?;
            Ok(transferred)
        }
        .await;

        let result = match result {
            Ok(transferred) => {
                progress.progress(transferred, Some(stat.size));
                Ok(transferred)
            }
            Err(error) => {
                if let Err(close_error) = close_handle(&cached.session, handle).await {
                    tracing::warn!(
                        event = "sftp",
                        operation = "close_handle",
                        connection = conn.get(),
                        direction = "download",
                        result = result_label(&Err::<(), _>(&close_error))
                    );
                }
                if let Err(remove_error) = tokio::fs::remove_file(&part).await {
                    tracing::warn!(
                        event = "sftp",
                        operation = "remove_part",
                        connection = conn.get(),
                        direction = "download",
                        result = remove_error.to_string()
                    );
                }
                Err(error)
            }
        };
        tracing::info!(
            event = "transfer",
            connection = conn.get(),
            direction = "download",
            bytes = result.unwrap_or(0),
            result = result_label(&result),
        );
        result
    }

    async fn upload(
        &self,
        conn: ConnectionId,
        local: &Path,
        remote: &RemotePath,
        progress: &dyn TransferProgress,
        cancel: &CancelToken,
    ) -> Result<u64, TransferError> {
        let metadata = tokio::fs::metadata(local).await.map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                TransferError::NotFound
            } else {
                TransferError::LocalIo
            }
        })?;
        match self.stat(conn, remote).await {
            Ok(_) => return Err(TransferError::AlreadyExists),
            Err(FsError::NotFound) => {}
            Err(error) => return Err(map_fs_error(error)),
        }
        let part = remote_part(remote)?;
        let cached = self.cached(conn).await.map_err(map_fs_error)?;
        // Un `.part` ajeno se reporta sin borrarlo: la limpieza solo cubre el creado aquí.
        match self.stat(conn, &part).await {
            Ok(_) => return Err(TransferError::AlreadyExists),
            Err(FsError::NotFound) => {}
            Err(error) => return Err(map_fs_error(error)),
        }
        let mut source = tokio::fs::File::open(local)
            .await
            .map_err(|_| TransferError::LocalIo)?;
        let handle = open_handle(
            &cached.session,
            part.as_str(),
            OpenFlags::CREATE | OpenFlags::EXCLUDE | OpenFlags::WRITE,
        )
        .await?;

        let result: Result<u64, TransferError> = async {
            let transferred = copy_upload(
                &mut source,
                &cached.session,
                &handle,
                metadata.len(),
                cached.write_len,
                progress,
                cancel,
            )
            .await?;
            close_handle(&cached.session, handle.clone()).await?;
            cached
                .session
                .rename(part.as_str(), remote.as_str())
                .await
                .map_err(map_transfer_error)?;
            Ok(transferred)
        }
        .await;

        let result = match result {
            Ok(transferred) => {
                progress.progress(transferred, Some(metadata.len()));
                Ok(transferred)
            }
            Err(error) => {
                if let Err(close_error) = close_handle(&cached.session, handle).await {
                    tracing::warn!(
                        event = "sftp",
                        operation = "close_handle",
                        connection = conn.get(),
                        direction = "upload",
                        result = result_label(&Err::<(), _>(&close_error))
                    );
                }
                let part_log = part.clone();
                if let Err(remove_error) = self
                    .run(
                        conn,
                        "remove_part",
                        Some(part_log.as_str()),
                        |session| async move { session.remove(part.as_str()).await.map(|_| ()) },
                    )
                    .await
                {
                    tracing::warn!(
                        event = "sftp",
                        operation = "remove_part",
                        connection = conn.get(),
                        direction = "upload",
                        result = result_label(&Err::<(), _>(&remove_error))
                    );
                }
                Err(error)
            }
        };
        tracing::info!(
            event = "transfer",
            connection = conn.get(),
            direction = "upload",
            bytes = result.unwrap_or(0),
            result = result_label(&result),
        );
        result
    }
}

struct ReadSlot {
    offset: u64,
    len: u32,
    task: tokio::task::JoinHandle<Result<Vec<u8>, TransferError>>,
}

struct WriteSlot {
    len: u64,
    task: tokio::task::JoinHandle<Result<(), SftpClientError>>,
}

/// Copia el archivo remoto en bloques hacia el parcial local con ventana de peticiones.
async fn copy_download(
    session: &Arc<RawSftpSession>,
    handle: &SftpHandle,
    target: &mut tokio::fs::File,
    total: u64,
    read_len: u32,
    progress: &dyn TransferProgress,
    cancel: &CancelToken,
) -> Result<u64, TransferError> {
    let session = Arc::clone(session);
    let handle = handle.clone();
    copy_download_batches(
        move |offset, len| {
            let session = Arc::clone(&session);
            let handle = handle.clone();
            async move {
                session
                    .read(handle, offset, len)
                    .await
                    .map(|data| data.data)
                    .map_err(map_transfer_error)
            }
        },
        target,
        total,
        read_len,
        progress,
        cancel,
    )
    .await
}

/// Ventana de lecturas ordenadas por offset, independiente del transporte SFTP.
async fn copy_download_batches<F, Fut, W>(
    read_at: F,
    target: &mut W,
    total: u64,
    chunk_len: u32,
    progress: &dyn TransferProgress,
    cancel: &CancelToken,
) -> Result<u64, TransferError>
where
    F: Fn(u64, u32) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<Vec<u8>, TransferError>> + Send + 'static,
    W: AsyncWrite + Unpin,
{
    let chunk_len = chunk_len.max(1);
    let read_at = Arc::new(read_at);
    let mut inflight: VecDeque<ReadSlot> = VecDeque::with_capacity(TRANSFER_WINDOW);
    let mut next_offset = 0_u64;
    let mut transferred = 0_u64;
    let mut eof = false;

    while !eof || !inflight.is_empty() {
        if cancel.is_cancelled() {
            drain_reads(&mut inflight).await;
            return Err(TransferError::Cancelled);
        }

        while !eof && inflight.len() < TRANSFER_WINDOW {
            let remaining = total.saturating_sub(next_offset);
            if remaining == 0 {
                eof = true;
                break;
            }
            let len = remaining.min(u64::from(chunk_len)) as u32;
            let offset = next_offset;
            next_offset += u64::from(len);
            let read_at = Arc::clone(&read_at);
            let task = tokio::spawn(async move { read_at(offset, len).await });
            inflight.push_back(ReadSlot { offset, len, task });
        }

        let Some(slot) = inflight.pop_front() else {
            break;
        };
        let bytes = slot.task.await.map_err(|error| {
            tracing::warn!(
                event = "transfer",
                operation = "download",
                result = "read_task_failed",
                error = %error,
            );
            TransferError::Remote
        })??;
        let read = bytes.len();

        if read == 0 {
            if transferred >= total {
                drain_reads(&mut inflight).await;
                break;
            }
            // El servidor devolvió EOF antes del tamaño anunciado: el remoto se encogió.
            // Descartamos las lecturas pendientes para no escribir offsets fuera de orden
            // ni revalidar el mismo offset en un bucle infinito.
            tracing::warn!(
                event = "transfer",
                operation = "download",
                result = "remote_shrank",
                offset = slot.offset,
            );
            drain_reads(&mut inflight).await;
            return Err(TransferError::Remote);
        }

        if (read as u32) < slot.len {
            // Lectura corta: reencolar lo que falta desde el hueco.
            next_offset = next_offset.min(slot.offset + read as u64);
            drain_reads(&mut inflight).await;
            eof = next_offset >= total;
        }

        target
            .write_all(&bytes)
            .await
            .map_err(|_| TransferError::LocalIo)?;
        transferred += read as u64;
        progress.progress(transferred, Some(total));

        if transferred >= total {
            drain_reads(&mut inflight).await;
            break;
        }
        if cancel.is_cancelled() {
            drain_reads(&mut inflight).await;
            return Err(TransferError::Cancelled);
        }
    }

    Ok(transferred)
}

/// Copia el archivo local en bloques hacia el parcial remoto con ventana de peticiones.
async fn copy_upload(
    source: &mut tokio::fs::File,
    session: &Arc<RawSftpSession>,
    handle: &SftpHandle,
    total: u64,
    write_len: u32,
    progress: &dyn TransferProgress,
    cancel: &CancelToken,
) -> Result<u64, TransferError> {
    let chunk_len = usize::try_from(write_len.max(1)).unwrap_or(TRANSFER_CHUNK);
    let mut inflight: VecDeque<WriteSlot> = VecDeque::with_capacity(TRANSFER_WINDOW);
    let mut next_offset = 0_u64;
    let mut transferred = 0_u64;
    let mut source_eof = false;

    loop {
        if cancel.is_cancelled() {
            drain_writes(&mut inflight).await;
            return Err(TransferError::Cancelled);
        }

        while !source_eof && inflight.len() < TRANSFER_WINDOW {
            let mut buffer = vec![0_u8; chunk_len];
            let read = source
                .read(&mut buffer)
                .await
                .map_err(|_| TransferError::LocalIo)?;
            if read == 0 {
                source_eof = true;
                break;
            }
            buffer.truncate(read);
            let offset = next_offset;
            next_offset += read as u64;
            let session = Arc::clone(session);
            let handle = handle.clone();
            let len = read as u64;
            let task =
                tokio::spawn(
                    async move { session.write(handle, offset, buffer).await.map(|_| ()) },
                );
            inflight.push_back(WriteSlot { len, task });
        }

        if inflight.is_empty() {
            break;
        }

        let Some(slot) = inflight.pop_front() else {
            break;
        };
        slot.task
            .await
            .map_err(|_| TransferError::Remote)?
            .map_err(map_transfer_error)?;
        transferred += slot.len;
        progress.progress(transferred.min(total), Some(total));

        if cancel.is_cancelled() {
            drain_writes(&mut inflight).await;
            return Err(TransferError::Cancelled);
        }
    }

    Ok(transferred)
}

async fn drain_reads(inflight: &mut VecDeque<ReadSlot>) {
    while let Some(slot) = inflight.pop_front() {
        if let Err(error) = slot.task.await {
            tracing::warn!(
                event = "sftp",
                operation = "drain_read",
                result = error.to_string()
            );
        }
    }
}

async fn drain_writes(inflight: &mut VecDeque<WriteSlot>) {
    while let Some(slot) = inflight.pop_front() {
        if let Err(error) = slot.task.await {
            tracing::warn!(
                event = "sftp",
                operation = "drain_write",
                result = error.to_string()
            );
        }
    }
}

async fn open_handle(
    session: &RawSftpSession,
    path: &str,
    flags: OpenFlags,
) -> Result<SftpHandle, TransferError> {
    session
        .open(path, flags, FileAttributes::default())
        .await
        .map(|h| h.handle)
        .map_err(map_transfer_error)
}

async fn close_handle(session: &RawSftpSession, handle: SftpHandle) -> Result<(), TransferError> {
    session
        .close(handle)
        .await
        .map(|_| ())
        .map_err(map_transfer_error)
}

async fn read_dir(
    session: &RawSftpSession,
    path: &str,
) -> Result<Vec<(String, FileAttributes)>, SftpClientError> {
    let handle = session.opendir(path).await?.handle;
    let mut out = Vec::new();
    loop {
        match session.readdir(handle.clone()).await {
            Ok(name) => {
                for file in name.files {
                    out.push((file.filename.to_string(), file.attrs));
                }
            }
            Err(SftpClientError::Status(status)) if status.status_code == StatusCode::Eof => break,
            Err(error) => {
                if let Err(close_error) = session.close(handle).await {
                    tracing::warn!(
                        event = "sftp",
                        operation = "close_readdir_handle",
                        result = result_label(&Err::<(), _>(&close_error))
                    );
                }
                return Err(error);
            }
        }
    }
    session.close(handle).await.map(|_| ())?;
    Ok(out)
}

fn file_type_is_dir(attrs: &FileAttributes) -> bool {
    attrs
        .permissions
        .is_some_and(|mode| mode & 0o170000 == 0o040000)
}

/// Nombre del parcial local, añadiendo `.part` sin perder la extensión original.
fn local_part(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(".part");
    PathBuf::from(name)
}

/// Nombre del parcial remoto, añadiendo `.part` a la ruta destino.
fn remote_part(path: &RemotePath) -> Result<RemotePath, TransferError> {
    RemotePath::new(&format!("{}.part", path.as_str())).map_err(|_| TransferError::Remote)
}

fn map_fs_error(error: FsError) -> TransferError {
    match error {
        FsError::NotFound => TransferError::NotFound,
        FsError::PermissionDenied => TransferError::PermissionDenied,
        FsError::Operation => TransferError::Remote,
    }
}

fn map_transfer_error(error: SftpClientError) -> TransferError {
    match map_sftp_error(error) {
        FsError::NotFound => TransferError::NotFound,
        FsError::PermissionDenied => TransferError::PermissionDenied,
        FsError::Operation => TransferError::Remote,
    }
}

/// Traduce un error SFTP a un error del dominio sin exponer rutas ni detalles.
fn map_sftp_error(error: SftpClientError) -> FsError {
    match error {
        SftpClientError::Status(status) => match status.status_code {
            StatusCode::NoSuchFile => FsError::NotFound,
            StatusCode::PermissionDenied => FsError::PermissionDenied,
            _ => FsError::Operation,
        },
        _ => FsError::Operation,
    }
}

/// Detecta errores que indican que el canal SFTP ya no está disponible.
fn channel_closed(error: &SftpClientError) -> bool {
    match error {
        SftpClientError::Status(status) => matches!(
            status.status_code,
            StatusCode::ConnectionLost | StatusCode::NoConnection
        ),
        SftpClientError::IO(_)
        | SftpClientError::Timeout
        | SftpClientError::UnexpectedBehavior(_) => true,
        SftpClientError::Limited(_) | SftpClientError::UnexpectedPacket => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    struct SilentProgress;

    impl TransferProgress for SilentProgress {
        fn progress(&self, _transferred: u64, _total: Option<u64>) {}
    }

    /// Ejecuta la ventana con lecturas simuladas y devuelve lo escrito localmente.
    async fn download_with<F, Fut>(
        total: u64,
        chunk: u32,
        read_at: F,
    ) -> (Result<u64, TransferError>, Vec<u8>)
    where
        F: Fn(u64, u32) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Vec<u8>, TransferError>> + Send + 'static,
    {
        let mut target = Vec::new();
        let result = copy_download_batches(
            read_at,
            &mut target,
            total,
            chunk,
            &SilentProgress,
            &CancelToken::default(),
        )
        .await;
        (result, target)
    }

    #[tokio::test]
    async fn should_fail_remote_without_looping_when_read_is_empty_before_total() {
        let download = download_with(256, 64, |offset, len| async move {
            if offset == 0 {
                return Ok(vec![b'A'; len as usize]);
            }
            Ok(Vec::new())
        });
        let (result, target) = tokio::time::timeout(Duration::from_millis(500), download)
            .await
            .expect("la descarga debe terminar sin bucle");

        assert!(matches!(result, Err(TransferError::Remote)));
        assert_eq!(target, vec![b'A'; 64]);
    }

    #[tokio::test]
    async fn should_discard_inflight_reads_when_one_returns_empty() {
        let empty_at_64 = Arc::new(AtomicBool::new(true));
        let (result, target) = download_with(256, 64, move |offset, len| {
            let empty_at_64 = Arc::clone(&empty_at_64);
            async move {
                if offset == 64 && empty_at_64.swap(false, Ordering::SeqCst) {
                    return Ok(Vec::new());
                }
                Ok(vec![b'A' + (offset / 64) as u8; len as usize])
            }
        })
        .await;

        assert!(matches!(result, Err(TransferError::Remote)));
        assert_eq!(target, vec![b'A'; 64]);
    }

    #[test]
    fn should_build_kpi_in_release_when_running_the_recipe() {
        let mut in_kpi = false;
        let command = include_str!("../../../justfile")
            .lines()
            .find(|line| {
                if line.trim_end() == "kpi:" {
                    in_kpi = true;
                    return false;
                }
                in_kpi && line.trim_start().starts_with("cargo test")
            })
            .expect("cargo test de la receta kpi");

        assert!(
            command.contains("--release"),
            "la receta kpi debe medir el perfil release: {command}"
        );
    }
}
