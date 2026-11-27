//! Errores de validación del dominio. Sin I/O.

/// Error de validación producido por los value objects del dominio.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DomainError {
    #[error("port must be between 1 and 65535")]
    InvalidPort,
    #[error("host must not be empty")]
    EmptyHost,
    #[error("host contains invalid characters")]
    InvalidHost,
    #[error("username must not be empty")]
    EmptyUsername,
    #[error("remote path must not be empty")]
    EmptyRemotePath,
    #[error("remote path must not contain NUL bytes")]
    InvalidRemotePath,
    #[error("key path must not be empty")]
    EmptyKeyPath,
    #[error("name must not be empty")]
    EmptyName,
    #[error("a folder cannot be its own parent")]
    FolderCycle,
    #[error("PTY size dimensions must be greater than zero")]
    InvalidPtySize,
}

#[cfg(test)]
mod tests {
    use super::DomainError;

    #[test]
    fn should_expose_actionable_messages_when_displaying_errors() {
        let cases = [
            (DomainError::InvalidPort, "port must be between 1 and 65535"),
            (DomainError::EmptyHost, "host must not be empty"),
            (DomainError::InvalidHost, "host contains invalid characters"),
            (DomainError::EmptyUsername, "username must not be empty"),
            (
                DomainError::EmptyRemotePath,
                "remote path must not be empty",
            ),
            (
                DomainError::InvalidRemotePath,
                "remote path must not contain NUL bytes",
            ),
            (DomainError::EmptyKeyPath, "key path must not be empty"),
            (DomainError::EmptyName, "name must not be empty"),
            (
                DomainError::FolderCycle,
                "a folder cannot be its own parent",
            ),
            (
                DomainError::InvalidPtySize,
                "PTY size dimensions must be greater than zero",
            ),
        ];

        for (error, message) in cases {
            assert_eq!(error.to_string(), message);
        }
    }
}
