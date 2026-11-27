#![cfg(feature = "integration")]

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use russh::keys::HashAlg;
use russh::server::{Auth, Handler as ServerHandler, run_stream};
use tokio::net::TcpListener;
use tokio::sync::{Notify, oneshot};
use tokio::task::JoinHandle;
use tuki_adapters::key_loader::RusshKeyLoader;
use tuki_adapters::ssh_transport::RusshTransport;
use tuki_core::auth::KeyRef;
use tuki_core::host::{Host, Hostname, Port, Username};
use tuki_core::port_errors::TransportError;
use tuki_core::ports::{
    Connected, Credential, KeyLoader, PtySink, PtySize, SecretString, SshTransport,
};

struct NoopSink;

impl PtySink for NoopSink {
    fn data(&mut self, _bytes: &[u8]) {}
    fn closed(&mut self, _exit_status: Option<u32>) {}
}

/// Estado compartido del sink de prueba: bytes acumulados y cierre observado.
#[derive(Clone)]
struct SinkState {
    bytes: Arc<Mutex<Vec<u8>>>,
    closed: Arc<Mutex<Option<Option<u32>>>>,
    notify: Arc<Notify>,
}

impl SinkState {
    fn new() -> Self {
        Self {
            bytes: Arc::new(Mutex::new(Vec::new())),
            closed: Arc::new(Mutex::new(None)),
            notify: Arc::new(Notify::new()),
        }
    }
}

struct RecordingSink {
    state: SinkState,
}

impl RecordingSink {
    fn new() -> (Self, SinkState) {
        let state = SinkState::new();
        (
            Self {
                state: state.clone(),
            },
            state,
        )
    }
}

impl PtySink for RecordingSink {
    fn data(&mut self, bytes: &[u8]) {
        self.state.bytes.lock().unwrap().extend_from_slice(bytes);
        self.state.notify.notify_one();
    }

    fn closed(&mut self, exit_status: Option<u32>) {
        *self.state.closed.lock().unwrap() = Some(exit_status);
        self.state.notify.notify_one();
    }
}

async fn wait_for_output(state: &SinkState, needle: &[u8]) -> Vec<u8> {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let notified = state.notify.notified();
            tokio::pin!(notified);
            {
                let bytes = state.bytes.lock().unwrap();
                if bytes.windows(needle.len()).any(|window| window == needle) {
                    return bytes.clone();
                }
            }
            notified.await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("la salida del PTY no contenía {needle:?} en 5 s"))
}

async fn wait_for_close(state: &SinkState) -> Option<u32> {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let notified = state.notify.notified();
            tokio::pin!(notified);
            {
                if let Some(status) = *state.closed.lock().unwrap() {
                    return status;
                }
            }
            notified.await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("el sink no recibió closed en 5 s"))
}

fn fixture_path(relative: &str) -> String {
    format!(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures/{}"),
        relative
    )
}

fn test_host() -> Host {
    Host {
        hostname: Hostname::new("127.0.0.1").unwrap(),
        port: Port::new(2222).unwrap(),
        username: Username::new("tuki").unwrap(),
    }
}

async fn connected_host() -> (RusshTransport, Connected) {
    let transport = RusshTransport::new();
    let connected = transport.connect(&test_host()).await.unwrap();
    (transport, connected)
}

async fn authenticated_host() -> (RusshTransport, Connected) {
    let (transport, connected) = connected_host().await;
    let path = fixture_path("ed25519");
    let key = RusshKeyLoader
        .load(&KeyRef::new(&path).unwrap(), None)
        .await
        .unwrap();
    transport
        .authenticate(connected.id, Credential::PublicKey(key))
        .await
        .unwrap();
    (transport, connected)
}

fn expected_fingerprint() -> String {
    let contents = std::fs::read_to_string(fixture_path("host/ssh_host_ed25519_key.pub"))
        .expect("host key fixture");
    let base64 = contents.split_whitespace().nth(1).expect("base64 key");
    let key = russh::keys::parse_public_key_base64(base64).expect("valid public key");
    key.fingerprint(HashAlg::Sha256).to_string()
}

#[tokio::test]
async fn should_return_fixture_fingerprint_when_connecting() {
    let transport = RusshTransport::new();
    let connected = transport.connect(&test_host()).await.unwrap();
    assert_eq!(connected.fingerprint.as_str(), expected_fingerprint());
}

async fn authenticate_password(password: &str) -> Result<(), TransportError> {
    let (transport, connected) = connected_host().await;
    transport
        .authenticate(
            connected.id,
            Credential::Password(SecretString::new(password)),
        )
        .await
}

#[tokio::test]
async fn should_accept_valid_password_when_authenticating() {
    assert!(authenticate_password("tuki-test").await.is_ok());
}

#[tokio::test]
async fn should_reject_wrong_password_when_authenticating() {
    assert!(matches!(
        authenticate_password("wrong").await,
        Err(TransportError::AuthenticationRejected)
    ));
}

#[tokio::test]
async fn should_authenticate_with_fixture_keys_when_using_public_key() {
    let loader = RusshKeyLoader;
    for name in ["ed25519", "keys/rsa_pkcs8.pem", "keys/ecdsa_p256"] {
        let path = fixture_path(name);
        let key_ref = KeyRef::new(&path).unwrap();
        let private_key = loader.load(&key_ref, None).await.unwrap();
        let (transport, connected) = connected_host().await;
        assert!(
            transport
                .authenticate(connected.id, Credential::PublicKey(private_key))
                .await
                .is_ok(),
            "la llave {name} debería autenticar"
        );
    }
}

#[tokio::test]
async fn should_report_not_connected_when_using_a_closed_connection() {
    let (transport, connected) = connected_host().await;
    assert!(transport.close(connected.id).await.is_ok());
    assert!(matches!(
        transport.close(connected.id).await,
        Err(TransportError::NotConnected)
    ));
    let size = PtySize::new(24, 80).unwrap();
    assert!(matches!(
        transport
            .open_pty(connected.id, size, Box::new(NoopSink))
            .await,
        Err(TransportError::NotConnected)
    ));
}

#[tokio::test]
async fn should_return_connect_error_when_no_server_is_listening() {
    let mut host = test_host();
    host.port = Port::new(1).unwrap();
    let result = RusshTransport::new().connect(&host).await;
    assert!(matches!(result, Err(TransportError::Connect)));
}

#[tokio::test]
async fn should_return_connect_error_quickly_when_handshake_exceeds_timeout() {
    let mut host = test_host();
    host.hostname = Hostname::new("10.255.255.1").unwrap();
    let transport = RusshTransport::with_connect_timeout(Duration::from_millis(500));
    let started = Instant::now();
    let result = transport.connect(&host).await;
    assert!(matches!(result, Err(TransportError::Connect)));
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "connect tardó {:?} con timeout de 500 ms",
        started.elapsed()
    );
}

#[tokio::test]
async fn should_return_connect_error_when_server_never_sends_banner() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = tokio::spawn(async move {
        let (_stream, _) = listener.accept().await.unwrap();
        std::future::pending::<()>().await;
    });
    let mut host = test_host();
    host.port = Port::new(port).unwrap();
    let transport = RusshTransport::with_connect_timeout(Duration::from_millis(200));
    let started = Instant::now();
    let result = transport.connect(&host).await;
    assert!(matches!(result, Err(TransportError::Connect)));
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "connect tardó {:?} con timeout de 200 ms",
        started.elapsed()
    );
    server.abort();
}

/// Servidor de prueba que completa el handshake pero no responde al password hasta que se libera.
struct StallingAuth {
    started: Option<oneshot::Sender<()>>,
    release: oneshot::Receiver<()>,
}

impl ServerHandler for StallingAuth {
    type Error = russh::Error;

    async fn auth_password(&mut self, _user: &str, _password: &str) -> Result<Auth, Self::Error> {
        if let Some(started) = self.started.take() {
            let _ = started.send(());
        }
        let _ = (&mut self.release).await;
        Ok(Auth::Accept)
    }
}

async fn spawn_stalling_server(
    listener: TcpListener,
) -> (
    u16,
    oneshot::Receiver<()>,
    oneshot::Sender<()>,
    JoinHandle<()>,
) {
    let port = listener.local_addr().expect("dirección local").port();
    let (started_tx, started_rx) = oneshot::channel();
    let (release_tx, release_rx) = oneshot::channel();
    let config = Arc::new(russh::server::Config {
        keys: vec![
            russh::keys::load_secret_key(fixture_path("host/ssh_host_ed25519_key"), None)
                .expect("host key del fixture"),
        ],
        auth_rejection_time: Duration::ZERO,
        ..Default::default()
    });
    let task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("aceptar cliente");
        let handler = StallingAuth {
            started: Some(started_tx),
            release: release_rx,
        };
        if let Ok(session) = run_stream(config, stream, handler).await {
            let _ = session.await;
        }
    });
    (port, started_rx, release_tx, task)
}

#[tokio::test]
async fn should_close_another_connection_when_authentication_is_stalled() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let (port, started, release, server) = spawn_stalling_server(listener).await;
    let transport = RusshTransport::new();
    let mut stalled_host = test_host();
    stalled_host.port = Port::new(port).unwrap();
    let a = transport.connect(&stalled_host).await.unwrap();
    let b = transport.connect(&test_host()).await.unwrap();

    let auth = transport.authenticate(a.id, Credential::Password(SecretString::new("tuki-test")));
    tokio::pin!(auth);
    tokio::select! {
        result = &mut auth => panic!("autenticación terminó antes de liberar el servidor: {result:?}"),
        signal = started => assert!(signal.is_ok()),
    }

    let closed = tokio::time::timeout(Duration::from_secs(3), transport.close(b.id)).await;
    assert!(
        matches!(closed, Ok(Ok(()))),
        "close(B) no debe esperar a authenticate(A)"
    );

    release.send(()).expect("liberar autenticación");
    assert!(auth.await.is_ok());
    assert!(transport.close(a.id).await.is_ok());
    server.abort();
}

#[tokio::test]
async fn should_connect_with_ipv6_hostname_when_ipv6_is_available() {
    let listener = match TcpListener::bind(("::1", 0)).await {
        Ok(listener) => listener,
        Err(_) => return, // entorno sin IPv6: se omite
    };
    let (port, started, release, server) = spawn_stalling_server(listener).await;
    let host = Host {
        hostname: Hostname::new("::1").unwrap(),
        port: Port::new(port).unwrap(),
        username: Username::new("tuki").unwrap(),
    };
    let connected = RusshTransport::new().connect(&host).await.unwrap();
    assert_eq!(connected.fingerprint.as_str(), expected_fingerprint());
    drop(started);
    drop(release);
    server.abort();
}

#[tokio::test]
async fn should_receive_echo_when_writing_to_pty() {
    let (transport, connected) = authenticated_host().await;
    let (sink, state) = RecordingSink::new();
    let channel = transport
        .open_pty(connected.id, PtySize::new(24, 80).unwrap(), Box::new(sink))
        .await
        .unwrap();

    transport.write(channel, b"echo ok\n").await.unwrap();

    let output = wait_for_output(&state, b"ok").await;
    assert!(String::from_utf8_lossy(&output).contains("ok"));
    assert!(transport.close(connected.id).await.is_ok());
}

#[tokio::test]
async fn should_report_exit_status_when_shell_exits() {
    let (transport, connected) = authenticated_host().await;
    let (sink, state) = RecordingSink::new();
    let channel = transport
        .open_pty(connected.id, PtySize::new(24, 80).unwrap(), Box::new(sink))
        .await
        .unwrap();

    transport.write(channel, b"exit\n").await.unwrap();

    assert_eq!(wait_for_close(&state).await, Some(0));
    assert!(transport.close(connected.id).await.is_ok());
}

#[tokio::test]
async fn should_observe_new_size_when_resizing_pty() {
    let (transport, connected) = authenticated_host().await;
    let (sink, state) = RecordingSink::new();
    let channel = transport
        .open_pty(connected.id, PtySize::new(24, 80).unwrap(), Box::new(sink))
        .await
        .unwrap();

    transport
        .resize(channel, PtySize::new(40, 120).unwrap())
        .await
        .unwrap();
    transport.write(channel, b"stty size\n").await.unwrap();

    let output = wait_for_output(&state, b"40 120").await;
    assert!(String::from_utf8_lossy(&output).contains("40 120"));
    assert!(transport.close(connected.id).await.is_ok());
}

#[tokio::test]
async fn should_reject_write_when_channel_is_closed() {
    let (transport, connected) = authenticated_host().await;
    let (sink, state) = RecordingSink::new();
    let channel = transport
        .open_pty(connected.id, PtySize::new(24, 80).unwrap(), Box::new(sink))
        .await
        .unwrap();

    transport.write(channel, b"exit\n").await.unwrap();
    assert_eq!(wait_for_close(&state).await, Some(0));
    assert!(matches!(
        transport.write(channel, b"echo ok\n").await,
        Err(TransportError::NotConnected)
    ));
    assert!(transport.close(connected.id).await.is_ok());
}

#[tokio::test]
async fn should_notify_close_when_connection_is_closed_with_open_pty() {
    let (transport, connected) = authenticated_host().await;
    let (sink, state) = RecordingSink::new();
    transport
        .open_pty(connected.id, PtySize::new(24, 80).unwrap(), Box::new(sink))
        .await
        .unwrap();

    transport.close(connected.id).await.unwrap();

    let _ = wait_for_close(&state).await;
}

#[tokio::test]
async fn should_share_connections_when_closing_through_a_transport_clone() {
    let transport = RusshTransport::new();
    let clone = transport.clone();
    let connected = clone.connect(&test_host()).await.unwrap();

    assert!(transport.close(connected.id).await.is_ok());
}
