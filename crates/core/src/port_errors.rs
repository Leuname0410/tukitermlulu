//! Errores de los puertos. Nunca contienen secretos.

/// Fallos de conexión, autenticación o canal SSH.
#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error("SSH connection failed")]
    Connect,
    #[error("SSH authentication failed")]
    Authentication,
    #[error("failed to open a PTY channel")]
    OpenPty,
    #[error("PTY resize failed")]
    Resize,
    #[error("failed to close the SSH connection")]
    Close,
    #[error("SSH transport I/O failed")]
    Io,
}

/// Fallos de operaciones remotas de archivos.
#[derive(Debug, thiserror::Error)]
pub enum FsError {
    #[error("remote path not found")]
    NotFound,
    #[error("permission denied on the remote file system")]
    PermissionDenied,
    #[error("remote file system operation failed")]
    Operation,
}
