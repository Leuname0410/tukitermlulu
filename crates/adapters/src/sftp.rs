//! Sistema de archivos remoto sobre el subsistema SFTP de la conexión SSH compartida.

use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;

use russh_sftp::client::SftpSession;
use russh_sftp::client::error::Error as SftpClientError;
use russh_sftp::protocol::StatusCode;
use tokio::sync::Mutex;
use tuki_core::port_errors::FsError;
use tuki_core::ports::{ConnectionId, FileEntry, FileStat, RemoteFileSystem};
use tuki_core::remote_path::RemotePath;

use crate::ssh_transport::RusshTransport;

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
                Some(FileEntry {
                    path: path.join(&name).ok()?,
                    is_dir: entry.file_type().is_dir(),
                    size: entry.metadata().len(),
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
