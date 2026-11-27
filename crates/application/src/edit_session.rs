//! Casos de uso para crear y editar sesiones.

use tuki_core::auth::AuthMethod;
use tuki_core::port_errors::RepositoryError;
use tuki_core::ports::{SecretKey, SecretStore, SecretString, SessionRepository};
use tuki_core::session::Session;

use crate::session_errors::{PasswordChange, SessionDraft, SessionError};

/// Crea una sesión y, si corresponde, guarda su contraseña.
pub struct CreateSession<R: SessionRepository, S: SecretStore> {
    repo: R,
    secrets: S,
}

impl<R: SessionRepository, S: SecretStore> CreateSession<R, S> {
    /// Crea el caso de uso con los puertos que necesita.
    pub fn new(repo: R, secrets: S) -> Self {
        Self { repo, secrets }
    }

    /// Crea la sesión y guarda la contraseña solo cuando el auth lo pide.
    pub async fn execute(
        &self,
        draft: SessionDraft,
        password: Option<&SecretString>,
    ) -> Result<Session, SessionError> {
        let id = self.repo.next_session_id().await?;
        let session = Session::new(
            id,
            &draft.name,
            draft.host,
            draft.auth.clone(),
            draft.folder,
        )?;
        self.repo.save_session(&session).await?;

        if matches!(draft.auth, AuthMethod::Password { remember: true })
            && let Some(value) = password
        {
            self.secrets.set(&SecretKey { session: id }, value).await?;
        }

        Ok(session)
    }
}

/// Actualiza una sesión y sincroniza la contraseña guardada.
pub struct UpdateSession<R: SessionRepository, S: SecretStore> {
    repo: R,
    secrets: S,
}

impl<R: SessionRepository, S: SecretStore> UpdateSession<R, S> {
    /// Crea el caso de uso con los puertos que necesita.
    pub fn new(repo: R, secrets: S) -> Self {
        Self { repo, secrets }
    }

    /// Guarda los cambios de la sesión y aplica el cambio de contraseña.
    pub async fn execute(
        &self,
        session: &Session,
        password: PasswordChange,
    ) -> Result<(), SessionError> {
        let existing = self.repo.get_session(session.id).await?;
        if existing.is_none() {
            return Err(SessionError::Repository(RepositoryError::NotFound));
        }

        self.repo.save_session(session).await?;

        match &session.auth {
            AuthMethod::PublicKey { .. } | AuthMethod::Password { remember: false } => {
                self.secrets
                    .delete(&SecretKey {
                        session: session.id,
                    })
                    .await?;
            }
            AuthMethod::Password { remember: true } => match password {
                PasswordChange::Set(value) => {
                    self.secrets
                        .set(
                            &SecretKey {
                                session: session.id,
                            },
                            &value,
                        )
                        .await?;
                }
                PasswordChange::Forget => {
                    self.secrets
                        .delete(&SecretKey {
                            session: session.id,
                        })
                        .await?;
                }
                PasswordChange::Keep => {}
            },
        }

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
    use tuki_core::ports::{MockSecretStore, MockSessionRepository, SecretKey, SecretString};
    use tuki_core::session::{FolderId, Session, SessionId};

    use super::{CreateSession, UpdateSession};
    use crate::session_errors::{PasswordChange, SessionDraft, SessionError};

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

    fn draft(name: &str, auth: AuthMethod, folder: Option<FolderId>) -> SessionDraft {
        SessionDraft {
            name: name.to_owned(),
            host: host(),
            auth,
            folder,
        }
    }

    fn session(id: u64, auth: AuthMethod) -> Session {
        Session::new(
            SessionId::new(id),
            "srv",
            host(),
            auth,
            Some(FolderId::new(3)),
        )
        .unwrap()
    }

    #[test]
    fn should_set_password_once_when_creating_password_session_with_remember() {
        let mut repo = MockSessionRepository::new();
        let mut seq = Sequence::new();
        repo.expect_next_session_id()
            .once()
            .in_sequence(&mut seq)
            .returning(|| Box::pin(async { Ok(SessionId::new(10)) }));
        repo.expect_save_session()
            .once()
            .in_sequence(&mut seq)
            .withf(|s| s.id == SessionId::new(10) && s.name == "prod")
            .returning(|_| Box::pin(async { Ok(()) }));
        let mut secrets = MockSecretStore::new();
        secrets
            .expect_set()
            .once()
            .in_sequence(&mut seq)
            .withf(|key, value| {
                *key == SecretKey {
                    session: SessionId::new(10),
                } && value.expose() == "pw"
            })
            .returning(|_, _| Box::pin(async { Ok(()) }));

        let uc = CreateSession::new(repo, secrets);
        let result = block_on(uc.execute(
            draft(
                "prod",
                AuthMethod::Password { remember: true },
                Some(FolderId::new(3)),
            ),
            Some(&SecretString::new("pw")),
        ))
        .unwrap();
        assert_eq!(result.id, SessionId::new(10));
    }

    #[test]
    fn should_not_set_password_when_creating_password_session_without_remember() {
        let mut repo = MockSessionRepository::new();
        repo.expect_next_session_id()
            .once()
            .returning(|| Box::pin(async { Ok(SessionId::new(11)) }));
        repo.expect_save_session()
            .once()
            .returning(|_| Box::pin(async { Ok(()) }));
        let mut secrets = MockSecretStore::new();
        secrets.expect_set().times(0);
        secrets.expect_delete().times(0);

        let uc = CreateSession::new(repo, secrets);
        let result = block_on(uc.execute(
            draft("prod", AuthMethod::Password { remember: false }, None),
            Some(&SecretString::new("pw")),
        ))
        .unwrap();
        assert_eq!(result.id, SessionId::new(11));
    }

    #[test]
    fn should_not_set_password_when_creating_public_key_session() {
        let mut repo = MockSessionRepository::new();
        repo.expect_next_session_id()
            .once()
            .returning(|| Box::pin(async { Ok(SessionId::new(12)) }));
        repo.expect_save_session()
            .once()
            .returning(|_| Box::pin(async { Ok(()) }));
        let mut secrets = MockSecretStore::new();
        secrets.expect_set().times(0);

        let uc = CreateSession::new(repo, secrets);
        let auth = AuthMethod::PublicKey {
            key: tuki_core::auth::KeyRef::new("~/.ssh/id_ed25519").unwrap(),
            has_passphrase: false,
        };
        let result =
            block_on(uc.execute(draft("prod", auth, None), Some(&SecretString::new("pw"))))
                .unwrap();
        assert_eq!(result.id, SessionId::new(12));
    }

    #[test]
    fn should_save_session_and_keep_secret_when_updating_with_keep() {
        let mut repo = MockSessionRepository::new();
        repo.expect_get_session()
            .once()
            .with(eq(SessionId::new(7)))
            .returning(|_| {
                Box::pin(async { Ok(Some(session(7, AuthMethod::Password { remember: true }))) })
            });
        repo.expect_save_session()
            .once()
            .withf(|s| s.host.port.get() == 2222 && s.host.username.as_str() == "bob")
            .returning(|_| Box::pin(async { Ok(()) }));
        let mut secrets = MockSecretStore::new();
        secrets.expect_set().times(0);
        secrets.expect_delete().times(0);

        let uc = UpdateSession::new(repo, secrets);
        let updated = Session::new(
            SessionId::new(7),
            "srv",
            Host {
                hostname: Hostname::new("example.com").unwrap(),
                port: Port::new(2222).unwrap(),
                username: Username::new("bob").unwrap(),
            },
            AuthMethod::Password { remember: true },
            Some(FolderId::new(3)),
        )
        .unwrap();
        block_on(uc.execute(&updated, PasswordChange::Keep)).unwrap();
    }

    #[test]
    fn should_replace_stored_password_when_updating_with_set() {
        let mut repo = MockSessionRepository::new();
        repo.expect_get_session().once().returning(|_| {
            Box::pin(async { Ok(Some(session(7, AuthMethod::Password { remember: true }))) })
        });
        repo.expect_save_session()
            .once()
            .returning(|_| Box::pin(async { Ok(()) }));
        let mut secrets = MockSecretStore::new();
        secrets
            .expect_set()
            .once()
            .withf(|key, value| {
                *key == SecretKey {
                    session: SessionId::new(7),
                } && value.expose() == "new-pw"
            })
            .returning(|_, _| Box::pin(async { Ok(()) }));

        let uc = UpdateSession::new(repo, secrets);
        block_on(uc.execute(
            &session(7, AuthMethod::Password { remember: true }),
            PasswordChange::Set(SecretString::new("new-pw")),
        ))
        .unwrap();
    }

    #[test]
    fn should_delete_stored_password_when_updating_with_forget() {
        let mut repo = MockSessionRepository::new();
        repo.expect_get_session().once().returning(|_| {
            Box::pin(async { Ok(Some(session(7, AuthMethod::Password { remember: true }))) })
        });
        repo.expect_save_session()
            .once()
            .returning(|_| Box::pin(async { Ok(()) }));
        let mut secrets = MockSecretStore::new();
        secrets
            .expect_delete()
            .once()
            .with(eq(SecretKey {
                session: SessionId::new(7),
            }))
            .returning(|_| Box::pin(async { Ok(()) }));

        let uc = UpdateSession::new(repo, secrets);
        block_on(uc.execute(
            &session(7, AuthMethod::Password { remember: true }),
            PasswordChange::Forget,
        ))
        .unwrap();
    }

    #[test]
    fn should_delete_password_when_changing_auth_to_public_key() {
        let mut repo = MockSessionRepository::new();
        repo.expect_get_session().once().returning(|_| {
            Box::pin(async { Ok(Some(session(7, AuthMethod::Password { remember: true }))) })
        });
        repo.expect_save_session()
            .once()
            .returning(|_| Box::pin(async { Ok(()) }));
        let mut secrets = MockSecretStore::new();
        secrets
            .expect_delete()
            .once()
            .with(eq(SecretKey {
                session: SessionId::new(7),
            }))
            .returning(|_| Box::pin(async { Ok(()) }));
        secrets.expect_set().times(0);

        let uc = UpdateSession::new(repo, secrets);
        let auth = AuthMethod::PublicKey {
            key: tuki_core::auth::KeyRef::new("~/.ssh/id_ed25519").unwrap(),
            has_passphrase: false,
        };
        block_on(uc.execute(
            &session(7, auth),
            PasswordChange::Set(SecretString::new("ignored")),
        ))
        .unwrap();
    }

    #[test]
    fn should_delete_password_when_remember_is_disabled() {
        let mut repo = MockSessionRepository::new();
        repo.expect_get_session().once().returning(|_| {
            Box::pin(async { Ok(Some(session(7, AuthMethod::Password { remember: true }))) })
        });
        repo.expect_save_session()
            .once()
            .returning(|_| Box::pin(async { Ok(()) }));
        let mut secrets = MockSecretStore::new();
        secrets
            .expect_delete()
            .once()
            .with(eq(SecretKey {
                session: SessionId::new(7),
            }))
            .returning(|_| Box::pin(async { Ok(()) }));
        secrets.expect_set().times(0);

        let uc = UpdateSession::new(repo, secrets);
        block_on(uc.execute(
            &session(7, AuthMethod::Password { remember: false }),
            PasswordChange::Keep,
        ))
        .unwrap();
    }

    #[test]
    fn should_return_not_found_and_not_save_when_updating_missing_session() {
        let mut repo = MockSessionRepository::new();
        repo.expect_get_session()
            .once()
            .returning(|_| Box::pin(async { Ok(None) }));
        repo.expect_save_session().times(0);
        let mut secrets = MockSecretStore::new();
        secrets.expect_set().times(0);
        secrets.expect_delete().times(0);

        let uc = UpdateSession::new(repo, secrets);
        let err = block_on(uc.execute(
            &session(99, AuthMethod::Password { remember: true }),
            PasswordChange::Keep,
        ))
        .unwrap_err();
        assert!(matches!(
            err,
            SessionError::Repository(RepositoryError::NotFound)
        ));
    }

    #[test]
    fn should_propagate_secret_error_when_store_fails() {
        let mut repo = MockSessionRepository::new();
        repo.expect_next_session_id()
            .once()
            .returning(|| Box::pin(async { Ok(SessionId::new(10)) }));
        repo.expect_save_session()
            .once()
            .returning(|_| Box::pin(async { Ok(()) }));
        let mut secrets = MockSecretStore::new();
        secrets
            .expect_set()
            .once()
            .returning(|_, _| Box::pin(async { Err(SecretError::Unavailable) }));

        let uc = CreateSession::new(repo, secrets);
        let err = block_on(uc.execute(
            draft("prod", AuthMethod::Password { remember: true }, None),
            Some(&SecretString::new("topsecret-value")),
        ))
        .unwrap_err();
        assert!(matches!(
            err,
            SessionError::Secret(SecretError::Unavailable)
        ));
        let msg = format!("{err}");
        assert!(!msg.contains("topsecret-value"));
    }
}
