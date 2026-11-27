#![cfg(feature = "integration")]

use russh::keys::HashAlg;
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
