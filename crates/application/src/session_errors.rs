//! Tipos compartidos por los casos de uso de gestión de sesiones.

use tuki_core::auth::AuthMethod;
use tuki_core::error::DomainError;
use tuki_core::host::Host;
use tuki_core::port_errors::{RepositoryError, SecretError};
use tuki_core::ports::SecretString;
use tuki_core::session::FolderId;

/// Error de los casos de uso de gestión de sesiones.
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error(transparent)]
    Domain(#[from] DomainError),
    #[error(transparent)]
    Repository(#[from] RepositoryError),
    #[error(transparent)]
    Secret(#[from] SecretError),
}

/// Cambio solicitado sobre la contraseña guardada de una sesión.
pub enum PasswordChange {
    Keep,
    Set(SecretString),
    Forget,
}

impl std::fmt::Debug for PasswordChange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Keep => f.write_str("Keep"),
            Self::Set(_) => f.debug_tuple("Set").field(&"[REDACTED]").finish(),
            Self::Forget => f.write_str("Forget"),
        }
    }
}

/// Datos necesarios para crear o actualizar una sesión.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionDraft {
    pub name: String,
    pub host: Host,
    pub auth: AuthMethod,
    pub folder: Option<FolderId>,
}
