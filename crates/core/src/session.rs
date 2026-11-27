//! Entidades del gestor de sesiones: identificadores, carpetas y sesiones.

use crate::auth::AuthMethod;
use crate::error::DomainError;
use crate::host::Host;

/// Identificador opaco de una sesión.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SessionId(u64);

impl SessionId {
    pub fn new(value: u64) -> Self {
        Self(value)
    }

    pub fn get(self) -> u64 {
        self.0
    }
}

/// Identificador opaco de una carpeta.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FolderId(u64);

impl FolderId {
    pub fn new(value: u64) -> Self {
        Self(value)
    }

    pub fn get(self) -> u64 {
        self.0
    }
}

/// Carpeta del gestor de sesiones.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Folder {
    pub id: FolderId,
    pub name: String,
    pub parent: Option<FolderId>,
}

impl Folder {
    /// Crea una carpeta validando el nombre y que no sea su propio padre.
    pub fn new(id: FolderId, name: &str, parent: Option<FolderId>) -> Result<Self, DomainError> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(DomainError::EmptyName);
        }
        if parent == Some(id) {
            return Err(DomainError::FolderCycle);
        }
        Ok(Self {
            id,
            name: trimmed.to_owned(),
            parent,
        })
    }
}

/// Sesión guardada del gestor. No contiene secretos, solo el método de autenticación.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub id: SessionId,
    pub name: String,
    pub host: Host,
    pub auth: AuthMethod,
    pub folder: Option<FolderId>,
}

impl Session {
    /// Crea una sesión validando que el nombre no esté vacío.
    pub fn new(
        id: SessionId,
        name: &str,
        host: Host,
        auth: AuthMethod,
        folder: Option<FolderId>,
    ) -> Result<Self, DomainError> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(DomainError::EmptyName);
        }
        Ok(Self {
            id,
            name: trimmed.to_owned(),
            host,
            auth,
            folder,
        })
    }

    pub fn with_host(&self, host: Host) -> Self {
        Self {
            host,
            ..self.clone()
        }
    }

    pub fn with_auth(&self, auth: AuthMethod) -> Self {
        Self {
            auth,
            ..self.clone()
        }
    }

    /// Devuelve una sesión nueva con el nombre validado y recortado.
    pub fn rename(&self, name: &str) -> Result<Self, DomainError> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(DomainError::EmptyName);
        }
        Ok(Self {
            name: trimmed.to_owned(),
            ..self.clone()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{Folder, FolderId, Session, SessionId};
    use crate::auth::{AuthMethod, KeyRef};
    use crate::error::DomainError;
    use crate::host::{Host, Hostname, Port, Username};

    fn host(port: u16, user: &str) -> Host {
        Host {
            hostname: Hostname::new("example.com").expect("host válido"),
            port: Port::new(port).expect("puerto válido"),
            username: Username::new(user).expect("usuario válido"),
        }
    }

    fn sample_session(name: &str) -> Session {
        Session::new(
            SessionId::new(7),
            name,
            host(22, "alice"),
            AuthMethod::Password { remember: false },
            Some(FolderId::new(3)),
        )
        .expect("sesión válida")
    }

    #[test]
    fn should_expose_numeric_value_when_creating_ids() {
        assert_eq!(SessionId::new(7).get(), 7);
        assert_eq!(FolderId::new(3).get(), 3);
    }

    #[test]
    fn should_reject_blank_when_creating_folder() {
        for name in ["", "   "] {
            assert_eq!(
                Folder::new(FolderId::new(1), name, None),
                Err(DomainError::EmptyName)
            );
        }
    }

    #[test]
    fn should_reject_self_parent_when_creating_folder() {
        let id = FolderId::new(1);
        assert_eq!(
            Folder::new(id, "prod", Some(id)),
            Err(DomainError::FolderCycle)
        );
    }

    #[test]
    fn should_trim_name_when_creating_folder() {
        let folder = Folder::new(FolderId::new(1), "  prod  ", None).expect("carpeta válida");
        assert_eq!((folder.name.as_str(), folder.parent), ("prod", None));
    }

    #[test]
    fn should_reject_blank_when_creating_session() {
        for name in ["", "   "] {
            assert_eq!(
                Session::new(
                    SessionId::new(1),
                    name,
                    host(22, "alice"),
                    AuthMethod::Password { remember: false },
                    None,
                ),
                Err(DomainError::EmptyName)
            );
        }
    }

    #[test]
    fn should_trim_name_when_creating_session() {
        let session = sample_session("  prod  ");
        assert_eq!(session.name, "prod");
        assert_eq!(session.id.get(), 7);
        assert_eq!(session.folder, Some(FolderId::new(3)));
    }

    #[test]
    fn should_keep_id_when_changing_host() {
        let session = sample_session("prod");
        assert_eq!(
            session.with_host(host(2222, "bob")),
            Session {
                host: host(2222, "bob"),
                ..session.clone()
            }
        );
    }

    #[test]
    fn should_keep_id_when_changing_auth() {
        let session = sample_session("prod");
        let auth = AuthMethod::PublicKey {
            key: KeyRef::new("/home/bob/.ssh/id_ed25519").expect("ruta válida"),
            has_passphrase: true,
        };
        assert_eq!(
            session.with_auth(auth.clone()),
            Session {
                auth,
                ..session.clone()
            }
        );
    }

    #[test]
    fn should_reject_blank_when_renaming_session() {
        let session = sample_session("prod");
        assert_eq!(session.rename(""), Err(DomainError::EmptyName));
        assert_eq!(session.rename("   "), Err(DomainError::EmptyName));
    }

    #[test]
    fn should_trim_name_when_renaming_session() {
        let session = sample_session("prod");
        assert_eq!(
            session.rename("  staging  ").expect("nombre válido"),
            Session {
                name: "staging".to_owned(),
                ..session.clone()
            }
        );
    }

    #[test]
    fn should_enable_shell_integration_when_creating_session() {
        assert!(sample_session("prod").shell_integration);
    }

    #[test]
    fn should_keep_other_fields_when_changing_shell_integration() {
        let session = sample_session("prod");
        let disabled = session.with_shell_integration(false);
        assert!(!disabled.shell_integration);
        assert_eq!(
            disabled,
            Session {
                shell_integration: false,
                ..session.clone()
            }
        );
    }
}
