//! Sistema de archivos remoto sobre el subsistema SFTP de la conexión SSH compartida.

use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use russh_sftp::client::SftpSession;
use russh_sftp::client::error::Error as SftpClientError;
use russh_sftp::client::fs::File;
use russh_sftp::protocol::{FileAttributes, OpenFlags, StatusCode};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Mutex;
use tuki_core::port_errors::{FsError, TransferError};
use tuki_core::ports::{
    CancelToken, ConnectionId, FileEntry, FileStat, FileTransfer, RemoteFileSystem,
    TransferProgress,
};
use tuki_core::remote_path::RemotePath;

use crate::ssh_transport::RusshTransport;

/// Bloque fijo reutilizado para todas las transferencias (256 KiB).
const TRANSFER_CHUNK: usize = 256 * 1024;

/// Adaptador SFTP que reutiliza un canal por `ConnectionId`, sin nueva autenticación.
#[derive(Clone)]
pub struct RusshFileSystem {
    transport: RusshTransport,
    sessions: Arc<Mutex<HashMap<ConnectionId, Arc<SftpSession>>>>,
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
    async fn session(&self, conn: ConnectionId) -> Result<Arc<SftpSession>, FsError> {
        if let Some(session) = self.sessions.lock().await.get(&conn) {
            return Ok(Arc::clone(session));
        }
        let channel = self
            .transport
            .open_subsystem(conn, "sftp")
            .await
            .map_err(|_| FsError::Operation)?;
        let session = Arc::new(
            SftpSession::new(channel.into_stream())
                .await
                .map_err(map_sftp_error)?,
        );
        let mut sessions = self.sessions.lock().await;
        Ok(Arc::clone(sessions.entry(conn).or_insert(session)))
    }

    /// Descarta la sesión cacheada si el canal ya no sirve, permitiendo reabrirla.
    async fn invalidate(&self, conn: ConnectionId, session: &Arc<SftpSession>) {
        let mut sessions = self.sessions.lock().await;
        if sessions
            .get(&conn)
            .is_some_and(|cached| Arc::ptr_eq(cached, session))
        {
            sessions.remove(&conn);
        }
    }

    /// Ejecuta una operación SFTP y limpia la caché si el canal se cerró.
    async fn run<T, F, Fut>(&self, conn: ConnectionId, operation: F) -> Result<T, FsError>
    where
        F: FnOnce(Arc<SftpSession>) -> Fut,
        Fut: Future<Output = Result<T, SftpClientError>> + Send,
    {
        let session = self.session(conn).await?;
        match operation(Arc::clone(&session)).await {
            Ok(value) => Ok(value),
            Err(error) => {
                if channel_closed(&error) {
                    self.invalidate(conn, &session).await;
                }
                Err(map_sftp_error(error))
            }
        }
    }
}

impl RemoteFileSystem for RusshFileSystem {
    async fn list(&self, conn: ConnectionId, path: &RemotePath) -> Result<Vec<FileEntry>, FsError> {
        let entries = self
            .run(conn, |session| async move {
                session.read_dir(path.as_str()).await
            })
            .await?;
        Ok(entries
            .filter_map(|entry| {
                let name = entry.file_name().to_string();
                if name == "." || name == ".." {
                    return None;
                }
                let metadata = entry.metadata();
                Some(FileEntry {
                    path: path.join(&name).ok()?,
                    is_dir: metadata.file_type().is_dir(),
                    size: metadata.len(),
                    permissions: metadata.permissions.map(|mode| mode & 0o7777),
                })
            })
            .collect())
    }

    async fn stat(&self, conn: ConnectionId, path: &RemotePath) -> Result<FileStat, FsError> {
        let metadata = self
            .run(conn, |session| async move {
                session.metadata(path.as_str()).await
            })
            .await?;
        Ok(FileStat {
            size: metadata.len(),
            is_dir: metadata.file_type().is_dir(),
            permissions: metadata.permissions.map(|mode| mode & 0o7777),
        })
    }

    async fn rename(
        &self,
        conn: ConnectionId,
        from: &RemotePath,
        to: &RemotePath,
    ) -> Result<(), FsError> {
        self.run(conn, |session| async move {
            session.rename(from.as_str(), to.as_str()).await
        })
        .await
    }

    async fn remove(&self, conn: ConnectionId, path: &RemotePath) -> Result<(), FsError> {
        let stat = self.stat(conn, path).await?;
        self.run(conn, |session| async move {
            if stat.is_dir {
                session.remove_dir(path.as_str()).await
            } else {
                session.remove_file(path.as_str()).await
            }
        })
        .await
    }

    async fn mkdir(&self, conn: ConnectionId, path: &RemotePath) -> Result<(), FsError> {
        self.run(conn, |session| async move {
            session.create_dir(path.as_str()).await
        })
        .await
    }

    async fn chmod(&self, conn: ConnectionId, path: &RemotePath, mode: u32) -> Result<(), FsError> {
        // Solo se envían los bits de permiso: el tipo de archivo no se toca.
        let metadata = FileAttributes {
            permissions: Some(mode & 0o7777),
            ..FileAttributes::default()
        };
        self.run(conn, |session| async move {
            session.set_metadata(path.as_str(), metadata).await
        })
        .await
    }

    async fn home(&self, conn: ConnectionId) -> Result<RemotePath, FsError> {
        let home = self
            .run(
                conn,
                |session| async move { session.canonicalize(".").await },
            )
            .await?;
        RemotePath::new(&home).map_err(|_| FsError::Operation)
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
        let session = self.session(conn).await.map_err(map_fs_error)?;
        let mut source = session
            .open(remote.as_str())
            .await
            .map_err(map_transfer_error)?;
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
            let transferred =
                copy_download(&mut source, &mut target, stat.size, progress, cancel).await?;
            target.flush().await.map_err(|_| TransferError::LocalIo)?;
            drop(target);
            tokio::fs::rename(&part, local)
                .await
                .map_err(|_| TransferError::LocalIo)?;
            Ok(transferred)
        }
        .await;

        match result {
            Ok(transferred) => {
                progress.progress(transferred, Some(stat.size));
                Ok(transferred)
            }
            Err(error) => {
                let _ = tokio::fs::remove_file(&part).await;
                Err(error)
            }
        }
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
        let session = self.session(conn).await.map_err(map_fs_error)?;
        // Un `.part` ajeno se reporta sin borrarlo: la limpieza solo cubre el creado aquí.
        match self.stat(conn, &part).await {
            Ok(_) => return Err(TransferError::AlreadyExists),
            Err(FsError::NotFound) => {}
            Err(error) => return Err(map_fs_error(error)),
        }
        let mut source = tokio::fs::File::open(local)
            .await
            .map_err(|_| TransferError::LocalIo)?;
        let mut target = session
            .open_with_flags(
                part.as_str(),
                OpenFlags::CREATE | OpenFlags::EXCLUDE | OpenFlags::WRITE,
            )
            .await
            .map_err(map_transfer_error)?;

        let result: Result<u64, TransferError> = async {
            let transferred =
                copy_upload(&mut source, &mut target, metadata.len(), progress, cancel).await?;
            target.close().await.map_err(|_| TransferError::Remote)?;
            session
                .rename(part.as_str(), remote.as_str())
                .await
                .map_err(map_transfer_error)?;
            Ok(transferred)
        }
        .await;

        match result {
            Ok(transferred) => {
                progress.progress(transferred, Some(metadata.len()));
                Ok(transferred)
            }
            Err(error) => {
                let _ = self
                    .run(conn, |session| async move {
                        session.remove_file(part.as_str()).await
                    })
                    .await;
                Err(error)
            }
        }
    }
}

/// Copia el archivo remoto en bloques hacia el parcial local.
async fn copy_download(
    source: &mut File,
    target: &mut tokio::fs::File,
    total: u64,
    progress: &dyn TransferProgress,
    cancel: &CancelToken,
) -> Result<u64, TransferError> {
    let mut buffer = vec![0_u8; TRANSFER_CHUNK];
    let mut transferred = 0_u64;
    loop {
        if cancel.is_cancelled() {
            return Err(TransferError::Cancelled);
        }
        let read = source
            .read(&mut buffer)
            .await
            .map_err(|_| TransferError::Remote)?;
        if read == 0 {
            break;
        }
        target
            .write_all(&buffer[..read])
            .await
            .map_err(|_| TransferError::LocalIo)?;
        transferred += read as u64;
        progress.progress(transferred, Some(total));
        if cancel.is_cancelled() {
            return Err(TransferError::Cancelled);
        }
    }
    Ok(transferred)
}

/// Copia el archivo local en bloques hacia el parcial remoto.
async fn copy_upload(
    source: &mut tokio::fs::File,
    target: &mut File,
    total: u64,
    progress: &dyn TransferProgress,
    cancel: &CancelToken,
) -> Result<u64, TransferError> {
    let mut buffer = vec![0_u8; TRANSFER_CHUNK];
    let mut transferred = 0_u64;
    loop {
        if cancel.is_cancelled() {
            return Err(TransferError::Cancelled);
        }
        let read = source
            .read(&mut buffer)
            .await
            .map_err(|_| TransferError::LocalIo)?;
        if read == 0 {
            break;
        }
        target
            .write_all(&buffer[..read])
            .await
            .map_err(|_| TransferError::Remote)?;
        transferred += read as u64;
        progress.progress(transferred, Some(total));
        if cancel.is_cancelled() {
            return Err(TransferError::Cancelled);
        }
    }
    Ok(transferred)
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
