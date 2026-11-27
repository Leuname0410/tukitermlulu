//! Caso de uso para orquestar una conexión SSH completa.

use tuki_core::auth::AuthMethod;
use tuki_core::port_errors::{HostKeyError, KeyError, SecretError, TransportError};
use tuki_core::ports::{
    ChannelId, Connected, ConnectionId, Credential, HostKeyFingerprint, HostKeyStatus,
    HostKeyVerifier, KeyLoader, PtySink, PtySize, SecretKey, SecretStore, SecretString,
    SshTransport,
};
use tuki_core::session::Session;

/// Entrada del caso de uso de conexión.
pub struct ConnectRequest<'a> {
    pub session: &'a Session,
    /// Contraseña tecleada ahora; si falta y `remember`, se busca en el almacén.
    pub password: Option<&'a SecretString>,
    /// Passphrase de la llave, de la UI o de la memoria de la pestaña.
    pub passphrase: Option<&'a SecretString>,
    /// Huella aceptada explícitamente por el usuario (TOFU).
    pub approved_fingerprint: Option<&'a HostKeyFingerprint>,
    pub pty_size: PtySize,
    pub sink: Box<dyn PtySink>,
}

/// Resultado de la conexión, o lo que falta para completarla.
#[derive(Debug)]
pub enum ConnectOutcome {
    Connected {
        connection: ConnectionId,
        channel: ChannelId,
    },
    NeedsHostKeyApproval {
        fingerprint: HostKeyFingerprint,
    },
    NeedsPassword,
    NeedsPassphrase,
}

/// Errores de la orquestación. Nunca incluyen secretos.
#[derive(Debug, thiserror::Error)]
pub enum ConnectError {
    #[error("the host key has changed")]
    HostKeyChanged { fingerprint: HostKeyFingerprint },
    #[error(transparent)]
    Transport(#[from] TransportError),
    #[error(transparent)]
    Key(#[from] KeyError),
    #[error(transparent)]
    HostKey(#[from] HostKeyError),
    #[error(transparent)]
    Secret(#[from] SecretError),
}

/// Orquesta handshake, verificación TOFU, credencial, autenticación y PTY.
pub struct ConnectSession<T: SshTransport, V: HostKeyVerifier, K: KeyLoader, S: SecretStore> {
    transport: T,
    verifier: V,
    keys: K,
    secrets: S,
}

#[rustfmt::skip]
impl<T, V, K, S> ConnectSession<T, V, K, S>
where
    T: SshTransport,
    V: HostKeyVerifier,
    K: KeyLoader,
    S: SecretStore,
{
    /// Crea el caso de uso con los puertos que necesita.
    pub fn new(transport: T, verifier: V, keys: K, secrets: S) -> Self {
        Self { transport, verifier, keys, secrets }
    }

    /// Ejecuta la conexión indicada por la petición.
    pub async fn execute(&self, request: ConnectRequest<'_>) -> Result<ConnectOutcome, ConnectError> {
        let connected = self.transport.connect(&request.session.host).await?;
        let id = connected.id;
        let outcome = self.after_connect(connected, request).await;
        if !matches!(&outcome, Ok(ConnectOutcome::Connected { .. })) { let _ = self.transport.close(id).await; }
        outcome
    }

    /// Completa la conexión ya establecida; `execute` cierra si no hay éxito.
    async fn after_connect(&self, connected: Connected, request: ConnectRequest<'_>) -> Result<ConnectOutcome, ConnectError> {
        let Connected { id, fingerprint } = connected;
        match self.verifier.verify(&request.session.host, &fingerprint).await? {
            HostKeyStatus::Known => {}
            HostKeyStatus::Unknown if request.approved_fingerprint == Some(&fingerprint) => { self.verifier.trust(&request.session.host, &fingerprint).await?; }
            HostKeyStatus::Unknown => { return Ok(ConnectOutcome::NeedsHostKeyApproval { fingerprint }); }
            HostKeyStatus::Changed => { return Err(ConnectError::HostKeyChanged { fingerprint }); }
        }
        let credential = match &request.session.auth {
            AuthMethod::Password { remember } => {
                let password = match request.password {
                    Some(typed) => Some(SecretString::new(typed.expose())),
                    None if *remember => self.secrets.get(&SecretKey { session: request.session.id }).await?,
                    None => None,
                };
                match password { Some(password) => Credential::Password(password), None => return Ok(ConnectOutcome::NeedsPassword) }
            }
            AuthMethod::PublicKey { key, .. } => match self.keys.load(key, request.passphrase).await {
                Ok(key) => Credential::PublicKey(key),
                Err(KeyError::PassphraseRequired) => return Ok(ConnectOutcome::NeedsPassphrase),
                Err(error) => return Err(ConnectError::Key(error)),
            },
        };
        self.transport.authenticate(id, credential).await?;
        let channel = self.transport.open_pty(id, request.pty_size, request.sink).await?;
        Ok(ConnectOutcome::Connected { connection: id, channel })
    }
}

#[cfg(test)]
#[rustfmt::skip]
mod tests {
    use std::future::Future;
    use std::task::{Context, Poll, Waker};
    use mockall::Sequence;
    use tuki_core::auth::{AuthMethod, KeyRef};
    use tuki_core::host::{Host, Hostname, Port, Username};
    use tuki_core::port_errors::{HostKeyError, KeyError, TransportError};
    use tuki_core::ports::{MockHostKeyVerifier, MockKeyLoader, MockPtySink, MockSecretStore, MockSshTransport, PrivateKey};
    use tuki_core::session::{Session, SessionId};
    use super::*;

    fn block_on<F: Future>(future: F) -> F::Output { let mut context = Context::from_waker(Waker::noop()); let mut future = std::pin::pin!(future); match future.as_mut().poll(&mut context) { Poll::Ready(output) => output, Poll::Pending => panic!("future pending") } }
    fn conn() -> ConnectionId { ConnectionId::new(1) }
    fn chan() -> ChannelId { ChannelId::new(2) }
    fn fp() -> HostKeyFingerprint { HostKeyFingerprint::new("SHA256:abc").unwrap() }
    fn other_fp() -> HostKeyFingerprint { HostKeyFingerprint::new("SHA256:other").unwrap() }

    fn session(auth: AuthMethod) -> Session {
        let host = Host { hostname: Hostname::new("example.com").unwrap(), port: Port::default(), username: Username::new("alice").unwrap() };
        Session::new(SessionId::new(7), "srv", host, auth, None).unwrap()
    }
    fn password_session(remember: bool) -> Session { session(AuthMethod::Password { remember }) }
    fn key_session() -> Session { session(AuthMethod::PublicKey { key: KeyRef::new("~/.ssh/id_ed25519").unwrap(), has_passphrase: false }) }

    fn request<'a>(session: &'a Session, password: Option<&'a SecretString>, passphrase: Option<&'a SecretString>, approved: Option<&'a HostKeyFingerprint>) -> ConnectRequest<'a> {
        ConnectRequest { session, password, passphrase, approved_fingerprint: approved, pty_size: PtySize { rows: 24, cols: 80 }, sink: Box::new(MockPtySink::new()) }
    }

    fn empty_secrets() -> MockSecretStore { let mut secrets = MockSecretStore::new(); secrets.expect_get().times(0).returning(|_| Box::pin(async { Ok(None) })); secrets }

    /// Mocks configurados para comprobar el orden completo de la conexión.
    fn sequenced(status: HostKeyStatus, trust_times: usize, password: Option<&'static str>) -> (MockSshTransport, MockHostKeyVerifier, MockKeyLoader) {
        let mut seq = Sequence::new();
        let mut transport = MockSshTransport::new();
        let mut verifier = MockHostKeyVerifier::new();
        let mut keys = MockKeyLoader::new();
        transport.expect_connect().once().in_sequence(&mut seq).returning(|_| Box::pin(async { Ok(Connected { id: ConnectionId::new(1), fingerprint: fp() }) }));
        verifier.expect_verify().once().in_sequence(&mut seq).returning(move |_, _| Box::pin(async move { Ok(status) }));
        let trust = verifier.expect_trust().times(trust_times);
        if trust_times > 0 { trust.in_sequence(&mut seq); }
        trust.returning(|_, _| Box::pin(async { Ok(()) }));
        if password.is_none() { keys.expect_load().once().in_sequence(&mut seq).returning(|_, _| Box::pin(async { Ok(PrivateKey::new(vec![1, 2, 3])) })); }
        if let Some(expected) = password {
            transport.expect_authenticate().once().in_sequence(&mut seq).withf(move |_, credential| matches!(credential, Credential::Password(secret) if secret.expose() == expected)).returning(|_, _| Box::pin(async { Ok(()) }));
        } else {
            transport.expect_authenticate().once().in_sequence(&mut seq).withf(|_, credential| matches!(credential, Credential::PublicKey(_))).returning(|_, _| Box::pin(async { Ok(()) }));
        }
        transport.expect_open_pty().once().in_sequence(&mut seq).returning(|_, _, _| Box::pin(async { Ok(ChannelId::new(2)) }));
        transport.expect_close().times(0).returning(|_| Box::pin(async { Ok(()) }));
        (transport, verifier, keys)
    }

    /// Configuración de mocks para los casos que no requieren comprobar el orden.
    #[derive(Default)]
    struct Setup {
        status: Option<HostKeyStatus>,
        verify_error: bool,
        trust: usize,
        auth: usize,
        pty: usize,
        close: usize,
        get: usize,
        stored: Option<&'static str>,
        password: Option<&'static str>,
        key_error: Option<KeyError>,
        reject: bool,
        pty_fail: bool,
    }

    impl Setup {
        fn build(&self, session: &Session) -> ConnectSession<MockSshTransport, MockHostKeyVerifier, MockKeyLoader, MockSecretStore> {
            let mut transport = MockSshTransport::new();
            transport.expect_connect().once().returning(|_| Box::pin(async { Ok(Connected { id: ConnectionId::new(1), fingerprint: fp() }) }));
            let mut verifier = MockHostKeyVerifier::new();
            if self.verify_error { verifier.expect_verify().once().returning(|_, _| Box::pin(async { Err(HostKeyError::Io) })); }
            else { let status = self.status.unwrap_or(HostKeyStatus::Known); verifier.expect_verify().once().returning(move |_, _| Box::pin(async move { Ok(status) })); }
            verifier.expect_trust().times(self.trust).returning(|_, _| Box::pin(async { Ok(()) }));
            let is_password = matches!(&session.auth, AuthMethod::Password { .. });
            if self.reject { transport.expect_authenticate().times(self.auth).returning(|_, _| Box::pin(async { Err(TransportError::AuthenticationRejected) })); }
            else if is_password {
                let expected = self.password.unwrap_or("pw");
                transport.expect_authenticate().times(self.auth).returning(move |_, credential| { assert!(matches!(credential, Credential::Password(secret) if secret.expose() == expected)); Box::pin(async { Ok(()) }) });
            } else {
                transport.expect_authenticate().times(self.auth).returning(|_, credential| { assert!(matches!(credential, Credential::PublicKey(_))); Box::pin(async { Ok(()) }) });
            }
            if self.pty_fail { transport.expect_open_pty().times(self.pty).returning(|_, _, _| Box::pin(async { Err(TransportError::OpenPty) })); }
            else { transport.expect_open_pty().times(self.pty).returning(|_, _, _| Box::pin(async { Ok(ChannelId::new(2)) })); }
            transport.expect_close().times(self.close).returning(|_| Box::pin(async { Ok(()) }));
            let mut secrets = MockSecretStore::new();
            let stored = self.stored.map(str::to_owned);
            secrets.expect_get().times(self.get).returning(move |_| { let stored = stored.clone(); Box::pin(async move { Ok(stored.map(SecretString::new)) }) });
            let mut keys = MockKeyLoader::new();
            let key_error = self.key_error;
            keys.expect_load().times(if is_password { 0 } else { 1 }).returning(move |_, _| { Box::pin(async move { match key_error { Some(error) => Err(error), None => Ok(PrivateKey::new(vec![1, 2, 3])) } }) });
            ConnectSession::new(transport, verifier, keys, secrets)
        }
    }

    #[test]
    fn should_connect_with_typed_password_when_host_key_is_known() {
        let session = password_session(false); let password = SecretString::new("typed-secret");
        let (transport, verifier, keys) = sequenced(HostKeyStatus::Known, 0, Some("typed-secret"));
        let use_case = ConnectSession::new(transport, verifier, keys, empty_secrets());
        let outcome = block_on(use_case.execute(request(&session, Some(&password), None, None))).unwrap();
        assert!(matches!(outcome, ConnectOutcome::Connected { connection, channel } if connection == conn() && channel == chan()));
    }

    #[test]
    fn should_connect_with_key_when_host_key_is_known() {
        let session = key_session();
        let (transport, verifier, keys) = sequenced(HostKeyStatus::Known, 0, None);
        let use_case = ConnectSession::new(transport, verifier, keys, empty_secrets());
        let outcome = block_on(use_case.execute(request(&session, None, None, None))).unwrap();
        assert!(matches!(outcome, ConnectOutcome::Connected { .. }));
    }

    #[test]
    fn should_need_host_key_approval_when_fingerprint_is_unknown() {
        let session = password_session(false); let password = SecretString::new("pw");
        let use_case = Setup { status: Some(HostKeyStatus::Unknown), close: 1, ..Default::default() }.build(&session);
        let outcome = block_on(use_case.execute(request(&session, Some(&password), None, None))).unwrap();
        assert!(matches!(outcome, ConnectOutcome::NeedsHostKeyApproval { fingerprint } if fingerprint == fp()));
    }

    #[test]
    fn should_trust_and_connect_when_unknown_fingerprint_is_approved() {
        let session = password_session(false); let password = SecretString::new("pw"); let approved = fp();
        let (transport, verifier, _) = sequenced(HostKeyStatus::Unknown, 1, Some("pw"));
        let use_case = ConnectSession::new(transport, verifier, MockKeyLoader::new(), empty_secrets());
        let outcome = block_on(use_case.execute(request(&session, Some(&password), None, Some(&approved)))).unwrap();
        assert!(matches!(outcome, ConnectOutcome::Connected { .. }));
    }

    #[test]
    fn should_need_approval_when_approved_fingerprint_differs() {
        let session = password_session(false); let password = SecretString::new("pw"); let approved = other_fp();
        let use_case = Setup { status: Some(HostKeyStatus::Unknown), close: 1, ..Default::default() }.build(&session);
        let outcome = block_on(use_case.execute(request(&session, Some(&password), None, Some(&approved)))).unwrap();
        assert!(matches!(outcome, ConnectOutcome::NeedsHostKeyApproval { fingerprint } if fingerprint == fp()));
    }

    #[test]
    fn should_fail_without_trusting_when_host_key_changed_even_if_approved() {
        let session = password_session(false); let password = SecretString::new("pw"); let approved = fp();
        let use_case = Setup { status: Some(HostKeyStatus::Changed), close: 1, ..Default::default() }.build(&session);
        let error = block_on(use_case.execute(request(&session, Some(&password), None, Some(&approved)))).unwrap_err();
        assert!(matches!(&error, ConnectError::HostKeyChanged { fingerprint } if *fingerprint == fp()));
    }

    #[test]
    fn should_use_stored_password_when_remember_is_enabled() {
        let session = password_session(true);
        let use_case = Setup { auth: 1, pty: 1, get: 1, stored: Some("stored-secret"), password: Some("stored-secret"), ..Default::default() }.build(&session);
        let outcome = block_on(use_case.execute(request(&session, None, None, None))).unwrap();
        assert!(matches!(outcome, ConnectOutcome::Connected { .. }));
    }

    #[test]
    fn should_need_password_when_remember_is_enabled_but_no_secret_is_stored() {
        let session = password_session(true);
        let use_case = Setup { get: 1, close: 1, ..Default::default() }.build(&session);
        let outcome = block_on(use_case.execute(request(&session, None, None, None))).unwrap();
        assert!(matches!(outcome, ConnectOutcome::NeedsPassword));
    }

    #[test]
    fn should_need_password_without_consulting_store_when_remember_is_disabled() {
        let session = password_session(false);
        let use_case = Setup { close: 1, ..Default::default() }.build(&session);
        let outcome = block_on(use_case.execute(request(&session, None, None, None))).unwrap();
        assert!(matches!(outcome, ConnectOutcome::NeedsPassword));
    }

    #[test]
    fn should_need_passphrase_when_key_requires_it() {
        let session = key_session();
        let use_case = Setup { close: 1, key_error: Some(KeyError::PassphraseRequired), ..Default::default() }.build(&session);
        let outcome = block_on(use_case.execute(request(&session, None, None, None))).unwrap();
        assert!(matches!(outcome, ConnectOutcome::NeedsPassphrase));
    }

    #[test]
    fn should_fail_with_wrong_passphrase_error_when_key_load_rejects_passphrase() {
        let session = key_session(); let passphrase = SecretString::new("bad");
        let use_case = Setup { close: 1, key_error: Some(KeyError::WrongPassphrase), ..Default::default() }.build(&session);
        let error = block_on(use_case.execute(request(&session, None, Some(&passphrase), None))).unwrap_err();
        assert!(matches!(error, ConnectError::Key(KeyError::WrongPassphrase)));
    }

    #[test]
    fn should_fail_and_close_when_authentication_is_rejected() {
        let session = password_session(false); let password = SecretString::new("pw");
        let use_case = Setup { auth: 1, close: 1, reject: true, ..Default::default() }.build(&session);
        let error = block_on(use_case.execute(request(&session, Some(&password), None, None))).unwrap_err();
        assert!(matches!(error, ConnectError::Transport(TransportError::AuthenticationRejected)));
    }

    #[test]
    fn should_fail_and_close_when_opening_pty_fails() {
        let session = password_session(false); let password = SecretString::new("pw");
        let use_case = Setup { auth: 1, pty: 1, close: 1, pty_fail: true, ..Default::default() }.build(&session);
        let error = block_on(use_case.execute(request(&session, Some(&password), None, None))).unwrap_err();
        assert!(matches!(error, ConnectError::Transport(TransportError::OpenPty)));
    }

    #[test]
    fn should_not_leak_password_when_formatting_connect_error() {
        let session = password_session(false); let password = SecretString::new("topsecret-unique");
        let use_case = Setup { close: 1, verify_error: true, ..Default::default() }.build(&session);
        let error = block_on(use_case.execute(request(&session, Some(&password), None, None))).unwrap_err();
        assert!(!format!("{error}").contains("topsecret-unique"));
        assert!(matches!(error, ConnectError::HostKey(HostKeyError::Io)));
    }
}
