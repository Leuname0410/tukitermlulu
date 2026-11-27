//! Política de retención en memoria de la passphrase de la llave por pestaña.

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};

use tuki_core::auth::AuthMethod;
use tuki_core::port_errors::KeyError;
use tuki_core::ports::{HostKeyVerifier, KeyLoader, SecretStore, SecretString, SshTransport};

use crate::connect_session::{ConnectError, ConnectOutcome, ConnectRequest, ConnectSession};

/// Identificador de pestaña de la UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TabId(u64);

impl TabId {
    /// Crea un identificador de pestaña.
    pub fn new(value: u64) -> Self {
        Self(value)
    }

    /// Devuelve el valor numérico de la pestaña.
    pub fn get(self) -> u64 {
        self.0
    }
}

/// Passphrases retenidas solo en memoria del backend, nunca persistidas.
#[derive(Default)]
pub struct TabPassphrases {
    entries: Mutex<HashMap<TabId, SecretString>>,
}

impl TabPassphrases {
    /// Crea el almacén vacío.
    pub fn new() -> Self {
        Self::default()
    }

    /// Copia la passphrase retenida sin mantener el lock a través de un `await`.
    fn copy_of(&self, tab: TabId) -> Option<SecretString> {
        let entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        entries
            .get(&tab)
            .map(|secret| SecretString::new(secret.expose()))
    }

    /// Retiene una copia de la passphrase de la pestaña.
    fn remember(&self, tab: TabId, passphrase: SecretString) {
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        entries.insert(tab, passphrase);
    }

    /// Olvida la passphrase de la pestaña; al soltarla se hace zeroize.
    pub fn forget(&self, tab: TabId) {
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        entries.remove(&tab);
    }
}

/// Decorador que aplica la política de memoria por pestaña a una conexión.
pub struct ConnectTab<'a, T, V, K, S>
where
    T: SshTransport,
    V: HostKeyVerifier,
    K: KeyLoader,
    S: SecretStore,
{
    connect: ConnectSession<T, V, K, S>,
    passphrases: &'a TabPassphrases,
}

#[rustfmt::skip]
impl<'a, T, V, K, S> ConnectTab<'a, T, V, K, S>
where T: SshTransport, V: HostKeyVerifier, K: KeyLoader, S: SecretStore
{
    /// Crea el caso de uso con la sesión y la memoria compartida.
    pub fn new(connect: ConnectSession<T, V, K, S>, passphrases: &'a TabPassphrases) -> Self { Self { connect, passphrases } }

    /// Conecta aplicando la política de retención de passphrase.
    pub async fn execute(&self, tab: TabId, request: ConnectRequest<'_>) -> Result<ConnectOutcome, ConnectError> {
        let retained = match request.passphrase {
            Some(_) => None,
            None => self.passphrases.copy_of(tab),
        };
        let effective = request.passphrase.or(retained.as_ref());
        let ConnectRequest { session, password, approved_fingerprint, pty_size, sink, .. } = request;
        let uses_key = matches!(&session.auth, AuthMethod::PublicKey { .. });
        let result = self.connect.execute(ConnectRequest {
            session,
            password,
            passphrase: effective,
            approved_fingerprint,
            pty_size,
            sink,
        }).await;
        match &result {
            Ok(ConnectOutcome::Connected { .. }) if uses_key => {
                if let Some(secret) = effective {
                    self.passphrases.remember(tab, SecretString::new(secret.expose()));
                }
            }
            Ok(ConnectOutcome::NeedsPassphrase) => self.passphrases.forget(tab),
            Err(ConnectError::Key(KeyError::WrongPassphrase)) => self.passphrases.forget(tab),
            _ => {}
        }
        result
    }
}

#[cfg(test)]
#[rustfmt::skip]
mod tests {
    use std::future::Future;
    use std::sync::{Arc, Mutex};
    use std::task::{Context, Poll, Waker};
    use tuki_core::auth::{AuthMethod, KeyRef};
    use tuki_core::host::{Host, Hostname, Port, Username};
    use tuki_core::port_errors::KeyError;
    use tuki_core::ports::{ChannelId, Connected, ConnectionId, HostKeyFingerprint, HostKeyStatus, MockHostKeyVerifier, MockKeyLoader, MockPtySink, MockSecretStore, MockSshTransport, PrivateKey, PtySize, SecretString};
    use tuki_core::session::{Session, SessionId};
    use super::*;

    fn block_on<F: Future>(future: F) -> F::Output { let mut context = Context::from_waker(Waker::noop()); let mut future = std::pin::pin!(future); match future.as_mut().poll(&mut context) { Poll::Ready(output) => output, Poll::Pending => panic!("future pending") } }
    fn fp() -> HostKeyFingerprint { HostKeyFingerprint::new("SHA256:abc").unwrap() }

    fn session(auth: AuthMethod) -> Session {
        let host = Host { hostname: Hostname::new("example.com").unwrap(), port: Port::default(), username: Username::new("alice").unwrap() };
        Session::new(SessionId::new(7), "srv", host, auth, None).unwrap()
    }
    fn key_session() -> Session { session(AuthMethod::PublicKey { key: KeyRef::new("~/.ssh/id_ed25519").unwrap(), has_passphrase: true }) }
    fn password_session() -> Session { session(AuthMethod::Password { remember: false }) }
    fn request<'a>(session: &'a Session, password: Option<&'a SecretString>, passphrase: Option<&'a SecretString>) -> ConnectRequest<'a> {
        ConnectRequest { session, password, passphrase, approved_fingerprint: None, pty_size: PtySize { rows: 24, cols: 80 }, sink: Box::new(MockPtySink::new()) }
    }

    #[derive(Clone, Copy)]
    enum Behavior { Accept, Require(&'static str), Wrong }

    struct Harness {
        connect: ConnectSession<MockSshTransport, MockHostKeyVerifier, MockKeyLoader, MockSecretStore>,
        seen: Arc<Mutex<Vec<Option<String>>>>,
    }

    fn harness(status: HostKeyStatus, behavior: Behavior) -> Harness {
        let mut transport = MockSshTransport::new();
        transport.expect_connect().returning(|_| Box::pin(async { Ok(Connected { id: ConnectionId::new(1), fingerprint: fp() }) }));
        transport.expect_authenticate().returning(|_, _| Box::pin(async { Ok(()) }));
        transport.expect_open_pty().returning(|_, _, _| Box::pin(async { Ok(ChannelId::new(2)) }));
        transport.expect_close().returning(|_| Box::pin(async { Ok(()) }));
        let mut verifier = MockHostKeyVerifier::new();
        verifier.expect_verify().returning(move |_, _| Box::pin(async move { Ok(status) }));
        let seen = Arc::new(Mutex::new(Vec::new()));
        let capture = Arc::clone(&seen);
        let mut keys = MockKeyLoader::new();
        keys.expect_load().returning(move |_, passphrase| {
            capture.lock().unwrap().push(passphrase.map(|secret| secret.expose().to_owned()));
            let result = match behavior {
                Behavior::Accept => Ok(PrivateKey::new(vec![1, 2, 3])),
                Behavior::Require(expected) => match passphrase {
                    Some(secret) if secret.expose() == expected => Ok(PrivateKey::new(vec![1, 2, 3])),
                    _ => Err(KeyError::PassphraseRequired),
                },
                Behavior::Wrong => Err(KeyError::WrongPassphrase),
            };
            Box::pin(async move { result })
        });
        // Sin expect_set: cualquier escritura en SecretStore haría panic.
        let secrets = MockSecretStore::new();
        Harness { connect: ConnectSession::new(transport, verifier, keys, secrets), seen }
    }

    fn retained(memory: &TabPassphrases, tab: TabId) -> Option<String> {
        memory.copy_of(tab).map(|secret| secret.expose().to_owned())
    }

    #[test]
    fn should_reuse_retained_passphrase_when_reconnecting_same_tab_without_it() {
        let session = key_session();
        let memory = TabPassphrases::new();
        let tab = TabId::new(1);
        let typed = SecretString::new("first-pass");
        let first = harness(HostKeyStatus::Known, Behavior::Require("first-pass"));
        let outcome = block_on(ConnectTab::new(first.connect, &memory).execute(tab, request(&session, None, Some(&typed)))).unwrap();
        assert!(matches!(outcome, ConnectOutcome::Connected { .. }));
        let second = harness(HostKeyStatus::Known, Behavior::Require("first-pass"));
        let outcome = block_on(ConnectTab::new(second.connect, &memory).execute(tab, request(&session, None, None))).unwrap();
        assert!(matches!(outcome, ConnectOutcome::Connected { .. }));
        assert_eq!(*second.seen.lock().unwrap(), vec![Some("first-pass".to_owned())]);
    }

    #[test]
    fn should_need_passphrase_when_reconnecting_after_forget() {
        let session = key_session();
        let memory = TabPassphrases::new();
        let tab = TabId::new(1);
        let typed = SecretString::new("first-pass");
        let first = harness(HostKeyStatus::Known, Behavior::Require("first-pass"));
        let outcome = block_on(ConnectTab::new(first.connect, &memory).execute(tab, request(&session, None, Some(&typed)))).unwrap();
        assert!(matches!(outcome, ConnectOutcome::Connected { .. }));
        memory.forget(tab);
        assert!(retained(&memory, tab).is_none());
        let second = harness(HostKeyStatus::Known, Behavior::Require("first-pass"));
        let outcome = block_on(ConnectTab::new(second.connect, &memory).execute(tab, request(&session, None, None))).unwrap();
        assert!(matches!(outcome, ConnectOutcome::NeedsPassphrase));
        assert_eq!(*second.seen.lock().unwrap(), vec![None]);
    }

    #[test]
    fn should_forget_retained_passphrase_when_key_rejects_it() {
        let session = key_session();
        let memory = TabPassphrases::new();
        let tab = TabId::new(1);
        let typed = SecretString::new("first-pass");
        let bad = SecretString::new("bad-pass");
        let first = harness(HostKeyStatus::Known, Behavior::Require("first-pass"));
        let outcome = block_on(ConnectTab::new(first.connect, &memory).execute(tab, request(&session, None, Some(&typed)))).unwrap();
        assert!(matches!(outcome, ConnectOutcome::Connected { .. }));
        assert_eq!(retained(&memory, tab).as_deref(), Some("first-pass"));
        let second = harness(HostKeyStatus::Known, Behavior::Wrong);
        let error = block_on(ConnectTab::new(second.connect, &memory).execute(tab, request(&session, None, Some(&bad)))).unwrap_err();
        assert!(matches!(error, ConnectError::Key(KeyError::WrongPassphrase)));
        assert!(retained(&memory, tab).is_none());
        let third = harness(HostKeyStatus::Known, Behavior::Require("first-pass"));
        let outcome = block_on(ConnectTab::new(third.connect, &memory).execute(tab, request(&session, None, None))).unwrap();
        assert!(matches!(outcome, ConnectOutcome::NeedsPassphrase));
    }

    #[test]
    fn should_not_use_one_tabs_passphrase_for_another_tab() {
        let session = key_session();
        let memory = TabPassphrases::new();
        let typed = SecretString::new("first-pass");
        let first = harness(HostKeyStatus::Known, Behavior::Require("first-pass"));
        block_on(ConnectTab::new(first.connect, &memory).execute(TabId::new(1), request(&session, None, Some(&typed)))).unwrap();
        let other = harness(HostKeyStatus::Known, Behavior::Require("first-pass"));
        let outcome = block_on(ConnectTab::new(other.connect, &memory).execute(TabId::new(2), request(&session, None, None))).unwrap();
        assert!(matches!(outcome, ConnectOutcome::NeedsPassphrase));
        assert_eq!(*other.seen.lock().unwrap(), vec![None]);
        assert_eq!(retained(&memory, TabId::new(1)).as_deref(), Some("first-pass"));
    }

    #[test]
    fn should_retain_nothing_when_session_uses_password() {
        let session = password_session();
        let memory = TabPassphrases::new();
        let tab = TabId::new(1);
        let password = SecretString::new("pw");
        let h = harness(HostKeyStatus::Known, Behavior::Accept);
        let outcome = block_on(ConnectTab::new(h.connect, &memory).execute(tab, request(&session, Some(&password), None))).unwrap();
        assert!(matches!(outcome, ConnectOutcome::Connected { .. }));
        assert!(retained(&memory, tab).is_none());
    }

    #[test]
    fn should_retain_nothing_when_key_still_needs_passphrase() {
        let session = key_session();
        let memory = TabPassphrases::new();
        let tab = TabId::new(1);
        let h = harness(HostKeyStatus::Known, Behavior::Require("first-pass"));
        let outcome = block_on(ConnectTab::new(h.connect, &memory).execute(tab, request(&session, None, None))).unwrap();
        assert!(matches!(outcome, ConnectOutcome::NeedsPassphrase));
        assert!(retained(&memory, tab).is_none());
    }

    #[test]
    fn should_leave_memory_unchanged_when_host_key_needs_approval() {
        let session = key_session();
        let memory = TabPassphrases::new();
        let tab = TabId::new(1);
        let typed = SecretString::new("first-pass");
        let other = SecretString::new("second-pass");
        let first = harness(HostKeyStatus::Known, Behavior::Require("first-pass"));
        block_on(ConnectTab::new(first.connect, &memory).execute(tab, request(&session, None, Some(&typed)))).unwrap();
        let second = harness(HostKeyStatus::Unknown, Behavior::Require("second-pass"));
        let outcome = block_on(ConnectTab::new(second.connect, &memory).execute(tab, request(&session, None, Some(&other)))).unwrap();
        assert!(matches!(outcome, ConnectOutcome::NeedsHostKeyApproval { .. }));
        assert_eq!(retained(&memory, tab).as_deref(), Some("first-pass"));
    }

    #[test]
    fn should_expose_numeric_value_of_tab_id() {
        assert_eq!(TabId::new(42).get(), 42);
    }
}
