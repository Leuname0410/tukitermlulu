//! Pruebas del puente IPC del gestor de sesiones.

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::sync::Arc;
    use std::task::{Context, Poll, Waker};

    use tuki_adapters::secret_store::InMemorySecretStore;
    use tuki_adapters::session_repository::SqliteSessionRepository;
    use tuki_application::session_errors::SessionError;
    use tuki_core::auth::{AuthMethod, KeyRef};
    use tuki_core::error::DomainError;
    use tuki_core::host::{Host, Hostname, Port, Username};
    use tuki_core::port_errors::{RepositoryError, SecretError};
    use tuki_core::ports::{SecretKey, SecretStore, SecretString};
    use tuki_core::session::{Session, SessionId};
    use zeroize::Zeroizing;

    use super::*;

    fn block_on<F: Future>(future: F) -> F::Output {
        let mut context = Context::from_waker(Waker::noop());
        let mut future = std::pin::pin!(future);
        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => output,
            Poll::Pending => panic!("future pending"),
        }
    }

    #[derive(Clone, Default)]
    struct SharedSecrets(Arc<InMemorySecretStore>);

    impl SecretStore for SharedSecrets {
        async fn get(&self, key: &SecretKey) -> Result<Option<SecretString>, SecretError> {
            self.0.get(key).await
        }

        async fn set(&self, key: &SecretKey, value: &SecretString) -> Result<(), SecretError> {
            self.0.set(key, value).await
        }

        async fn delete(&self, key: &SecretKey) -> Result<(), SecretError> {
            self.0.delete(key).await
        }
    }

    fn input() -> SessionInput {
        SessionInput {
            name: "prod".to_owned(),
            host: "example.com".to_owned(),
            port: 22,
            username: "alice".to_owned(),
            auth: AuthDto::Password { remember: true },
            folder: None,
        }
    }

    fn session(id: u64, auth: AuthMethod) -> Session {
        let host = Host {
            hostname: Hostname::new("example.com").unwrap(),
            port: Port::new(22).unwrap(),
            username: Username::new("alice").unwrap(),
        };
        Session::new(SessionId::new(id), "prod", host, auth, None).unwrap()
    }

    #[test]
    fn should_reject_invalid_input_when_converting_to_session() {
        let mut empty_host = input();
        empty_host.host.clear();
        assert_eq!(
            to_session(empty_host, 1).unwrap_err().kind(),
            "invalid_input"
        );

        let mut zero_port = input();
        zero_port.port = 0;
        assert_eq!(
            to_session(zero_port, 1).unwrap_err().kind(),
            "invalid_input"
        );

        let mut empty_name = input();
        empty_name.name.clear();
        assert_eq!(
            to_session(empty_name, 1).unwrap_err().kind(),
            "invalid_input"
        );

        let mut empty_key = input();
        empty_key.auth = AuthDto::Key {
            path: String::new(),
            has_passphrase: false,
        };
        assert_eq!(
            to_session(empty_key, 1).unwrap_err().kind(),
            "invalid_input"
        );
    }

    #[test]
    fn should_hide_stored_password_when_mapping_session_to_dto() {
        let secrets = InMemorySecretStore::default();
        let stored = session(
            7,
            AuthMethod::Password {
                remember: true,
            },
        );
        block_on(
            secrets.set(
                &SecretKey {
                    session: SessionId::new(7),
                },
                &SecretString::new("stored-pw"),
            ),
        )
        .unwrap();

        let dto = block_on(to_dto_with_secrets(&stored, &secrets)).unwrap();
        assert!(dto.password_saved);
        assert!(!serde_json::to_string(&dto).unwrap().contains("stored-pw"));

        let key = session(
            8,
            AuthMethod::PublicKey {
                key: KeyRef::new("/home/alice/.ssh/id_ed25519").unwrap(),
                has_passphrase: true,
            },
        );
        let dto = block_on(to_dto_with_secrets(&key, &secrets)).unwrap();
        assert!(!dto.password_saved);
        assert!(matches!(
            dto.auth,
            AuthDto::Key {
                has_passphrase: true,
                ..
            }
        ));
    }

    #[test]
    fn should_map_every_session_error_when_command_fails() {
        let kind = |error| CommandError::from_session(error).kind().to_owned();
        assert_eq!(kind(SessionError::Domain(DomainError::EmptyName)), "invalid_input");
        assert_eq!(
            kind(SessionError::Repository(RepositoryError::NotFound)),
            "not_found"
        );
        assert_eq!(
            kind(SessionError::Repository(RepositoryError::Conflict)),
            "conflict"
        );
        assert_eq!(kind(SessionError::Repository(RepositoryError::Io)), "io");
        assert_eq!(kind(SessionError::Secret(SecretError::Unavailable)), "io");
        assert_eq!(kind(SessionError::Secret(SecretError::Io)), "io");
    }

    #[test]
    fn should_reflect_password_changes_when_creating_and_updating() {
        let repository = SqliteSessionRepository::in_memory().unwrap();
        let secrets = SharedSecrets::default();

        let created = block_on(create(
            repository.clone(),
            secrets.clone(),
            input(),
            Some(Zeroizing::new("pw".to_owned())),
        ))
        .unwrap();
        let listed = block_on(list(repository.clone(), secrets.clone())).unwrap();
        assert_eq!(listed.len(), 1);
        assert!(listed[0].password_saved);

        block_on(update(
            repository.clone(),
            secrets.clone(),
            created.id,
            input(),
            PasswordInput::Forget,
        ))
        .unwrap();
        let listed = block_on(list(repository.clone(), secrets.clone())).unwrap();
        assert!(!listed[0].password_saved);
    }
}
