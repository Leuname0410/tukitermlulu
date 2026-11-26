//! Entidades del gestor de sesiones: identificadores, carpetas y sesiones.

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
        assert_eq!(Folder::new(id, "prod", Some(id)), Err(DomainError::FolderCycle));
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
}
