//! Casos de uso para duplicar y borrar sesiones.

use tuki_core::port_errors::RepositoryError;
use tuki_core::ports::{SecretKey, SecretStore, SessionRepository};
use tuki_core::session::{Session, SessionId};

use crate::session_errors::SessionError;

/// Duplica una sesión guardada sin copiar secretos.
pub struct DuplicateSession<R: SessionRepository> {
    repo: R,
}

impl<R: SessionRepository> DuplicateSession<R> {
    /// Crea el caso de uso con el repositorio necesario.
    pub fn new(repo: R) -> Self {
        Self { repo }
    }

    /// Duplica la sesión con un id nuevo y nombre derivado.
    pub async fn execute(&self, id: SessionId) -> Result<Session, SessionError> {
        let existing = self
            .repo
            .get_session(id)
            .await?
            .ok_or(SessionError::Repository(RepositoryError::NotFound))?;
        let next_id = self.repo.next_session_id().await?;
        let copy = Session::new(
            next_id,
            &format!("{} (copia)", existing.name),
            existing.host,
            existing.auth,
            existing.folder,
        )?;
        self.repo.save_session(&copy).await?;
        Ok(copy)
    }
}

/// Borra una sesión y su secreto asociado.
pub struct DeleteSession<R: SessionRepository, S: SecretStore> {
    repo: R,
    secrets: S,
}

impl<R: SessionRepository, S: SecretStore> DeleteSession<R, S> {
    /// Crea el caso de uso con los puertos que necesita.
    pub fn new(repo: R, secrets: S) -> Self {
        Self { repo, secrets }
    }

    /// Borra la sesión y elimina su contraseña guardada.
    pub async fn execute(&self, id: SessionId) -> Result<(), SessionError> {
        self.repo.delete_session(id).await?;
        self.secrets.delete(&SecretKey { session: id }).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::task::{Context, Poll, Waker};

    use mockall::Sequence;
    use mockall::predicate::*;
    use tuki_core::auth::AuthMethod;
    use tuki_core::host::{Host, Hostname, Port, Username};
    use tuki_core::port_errors::{RepositoryError, SecretError};
    use tuki_core::ports::{MockSecretStore, MockSessionRepository, SecretKey};
    use tuki_core::session::{FolderId, Session, SessionId};

    use super::{DeleteSession, DuplicateSession};
    use crate::session_errors::SessionError;

    fn block_on<F: Future>(future: F) -> F::Output {
        let mut context = Context::from_waker(Waker::noop());
        let mut future = std::pin::pin!(future);
        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => output,
            Poll::Pending => panic!("future pending"),
        }
    }

    fn host() -> Host {
        Host {
            hostname: Hostname::new("example.com").unwrap(),
            port: Port::new(22).unwrap(),
            username: Username::new("alice").unwrap(),
        }
    }

    fn session(id: u64) -> Session {
        Session::new(
            SessionId::new(id),
            "prod",
            host(),
            AuthMethod::Password { remember: true },
            Some(FolderId::new(3)),
        )
        .unwrap()
    }

    #[test]
    fn should_duplicate_session_with_new_id_copy_name_and_same_data() {
        let mut repo = MockSessionRepository::new();
        let mut seq = Sequence::new();
        repo.expect_get_session()
            .once()
            .in_sequence(&mut seq)
            .with(eq(SessionId::new(7)))
            .returning(|_| Box::pin(async { Ok(Some(session(7))) }));
        repo.expect_next_session_id()
            .once()
            .in_sequence(&mut seq)
            .returning(|| Box::pin(async { Ok(SessionId::new(8)) }));
        repo.expect_save_session()
            .once()
            .in_sequence(&mut seq)
            .withf(|s| {
                s.id == SessionId::new(8)
                    && s.name == "prod (copia)"
                    && s.host == session(7).host
                    && s.auth == session(7).auth
                    && s.folder == Some(FolderId::new(3))
            })
            .returning(|_| Box::pin(async { Ok(()) }));

        let uc = DuplicateSession::new(repo);
        let copy = block_on(uc.execute(SessionId::new(7))).unwrap();
        assert_eq!(copy.id, SessionId::new(8));
        assert_eq!(copy.name, "prod (copia)");
        assert_eq!(copy.folder, Some(FolderId::new(3)));
    }

    #[test]
    fn should_copy_shell_integration_when_duplicating_session() {
        let mut repo = MockSessionRepository::new();
        repo.expect_get_session().once().returning(|_| {
            Box::pin(async { Ok(Some(session(7).with_shell_integration(false))) })
        });
        repo.expect_next_session_id()
            .once()
            .returning(|| Box::pin(async { Ok(SessionId::new(8)) }));
        repo.expect_save_session()
            .once()
            .withf(|copy| !copy.shell_integration)
            .returning(|_| Box::pin(async { Ok(()) }));

        let copy = block_on(DuplicateSession::new(repo).execute(SessionId::new(7))).unwrap();
        assert!(!copy.shell_integration);
    }

    #[test]
    fn should_return_not_found_when_duplicating_missing_session() {
        let mut repo = MockSessionRepository::new();
        repo.expect_get_session()
            .once()
            .returning(|_| Box::pin(async { Ok(None) }));
        repo.expect_next_session_id().times(0);
        repo.expect_save_session().times(0);

        let uc = DuplicateSession::new(repo);
        let err = block_on(uc.execute(SessionId::new(99))).unwrap_err();
        assert!(matches!(
            err,
            SessionError::Repository(RepositoryError::NotFound)
        ));
    }

    #[test]
    fn should_delete_session_and_secret_when_deleting() {
        let mut repo = MockSessionRepository::new();
        let mut seq = Sequence::new();
        repo.expect_delete_session()
            .once()
            .in_sequence(&mut seq)
            .with(eq(SessionId::new(7)))
            .returning(|_| Box::pin(async { Ok(()) }));
        let mut secrets = MockSecretStore::new();
        secrets
            .expect_delete()
            .once()
            .in_sequence(&mut seq)
            .with(eq(SecretKey {
                session: SessionId::new(7),
            }))
            .returning(|_| Box::pin(async { Ok(()) }));

        let uc = DeleteSession::new(repo, secrets);
        block_on(uc.execute(SessionId::new(7))).unwrap();
    }

    #[test]
    fn should_propagate_secret_error_when_delete_secret_fails() {
        let mut repo = MockSessionRepository::new();
        repo.expect_delete_session()
            .once()
            .returning(|_| Box::pin(async { Ok(()) }));
        let mut secrets = MockSecretStore::new();
        secrets
            .expect_delete()
            .once()
            .returning(|_| Box::pin(async { Err(SecretError::Io) }));

        let uc = DeleteSession::new(repo, secrets);
        let err = block_on(uc.execute(SessionId::new(7))).unwrap_err();
        assert!(matches!(err, SessionError::Secret(SecretError::Io)));
    }
}
