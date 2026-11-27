//! Errores de los puertos. Nunca contienen secretos.

/// Fallos de conexión, autenticación o canal SSH.
#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error("SSH connection failed")]
    Connect,
    #[error("SSH authentication failed")]
    Authentication,
    #[error("SSH credentials were rejected")]
    AuthenticationRejected,
    #[error("SSH connection is closed or unknown")]
    NotConnected,
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

/// Fallos al cargar una llave privada. El mensaje nunca incluye la ruta ni la passphrase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum KeyError {
    #[error("key passphrase is incorrect")]
    WrongPassphrase,
    #[error("key passphrase is required")]
    PassphraseRequired,
    #[error("unsupported private key format")]
    UnsupportedFormat,
    #[error("key file not found")]
    NotFound,
    #[error("key loading I/O failed")]
    Io,
}

/// Fallos del almacén de secretos del sistema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SecretError {
    #[error("secret store is unavailable on this system")]
    Unavailable,
    #[error("secret store I/O failed")]
    Io,
}

/// Fallos de verificación de la llave del host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum HostKeyError {
    #[error("invalid host key fingerprint")]
    InvalidFingerprint,
    #[error("host key verification I/O failed")]
    Io,
}

/// Fallos de persistencia de sesiones y carpetas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum RepositoryError {
    #[error("session repository entry not found")]
    NotFound,
    #[error("session repository entry conflicts with existing data")]
    Conflict,
    #[error("session repository I/O failed")]
    Io,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_use_fixed_messages_when_formatting_transport_errors() {
        assert_eq!(TransportError::Connect.to_string(), "SSH connection failed");
        assert_eq!(
            TransportError::Authentication.to_string(),
            "SSH authentication failed"
        );
        assert_eq!(
            TransportError::AuthenticationRejected.to_string(),
            "SSH credentials were rejected"
        );
        assert_eq!(
            TransportError::NotConnected.to_string(),
            "SSH connection is closed or unknown"
        );
        assert_eq!(
            TransportError::OpenPty.to_string(),
            "failed to open a PTY channel"
        );
        assert_eq!(TransportError::Resize.to_string(), "PTY resize failed");
        assert_eq!(
            TransportError::Close.to_string(),
            "failed to close the SSH connection"
        );
        assert_eq!(TransportError::Io.to_string(), "SSH transport I/O failed");
    }

    #[test]
    fn should_use_fixed_messages_when_formatting_key_errors() {
        assert_eq!(
            KeyError::WrongPassphrase.to_string(),
            "key passphrase is incorrect"
        );
        assert_eq!(
            KeyError::PassphraseRequired.to_string(),
            "key passphrase is required"
        );
        assert_eq!(
            KeyError::UnsupportedFormat.to_string(),
            "unsupported private key format"
        );
        assert_eq!(KeyError::NotFound.to_string(), "key file not found");
        assert_eq!(KeyError::Io.to_string(), "key loading I/O failed");
    }

    #[test]
    fn should_use_fixed_messages_when_formatting_secret_errors() {
        assert_eq!(
            SecretError::Unavailable.to_string(),
            "secret store is unavailable on this system"
        );
        assert_eq!(SecretError::Io.to_string(), "secret store I/O failed");
    }

    #[test]
    fn should_use_fixed_messages_when_formatting_host_key_errors() {
        assert_eq!(
            HostKeyError::InvalidFingerprint.to_string(),
            "invalid host key fingerprint"
        );
        assert_eq!(
            HostKeyError::Io.to_string(),
            "host key verification I/O failed"
        );
    }

    #[test]
    fn should_use_fixed_messages_when_formatting_repository_errors() {
        assert_eq!(
            RepositoryError::NotFound.to_string(),
            "session repository entry not found"
        );
        assert_eq!(
            RepositoryError::Conflict.to_string(),
            "session repository entry conflicts with existing data"
        );
        assert_eq!(
            RepositoryError::Io.to_string(),
            "session repository I/O failed"
        );
    }
}
