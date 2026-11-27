//! Adaptador SSH basado en `russh`: handshake, huella del host y autenticación.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use russh::client::{AuthResult, Config, Handle};
use russh::keys::{HashAlg, PrivateKeyWithHashAlg, PublicKeyOrCertificate};
use russh::{Disconnect, Error as RusshError};
use tokio::sync::Mutex;
use tuki_core::host::Host;
use tuki_core::port_errors::TransportError;
use tuki_core::ports::{
    ChannelId, Connected, ConnectionId, Credential, HostKeyFingerprint, PrivateKey, PtySink,
    PtySize, SshTransport,
};

struct FingerprintHandler {
    fingerprint: Arc<Mutex<Option<String>>>,
}

impl russh::client::Handler for FingerprintHandler {
    type Error = RusshError;

    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let observed = server_public_key
            .public_key()
            .fingerprint(HashAlg::Sha256)
            .to_string();
        *self.fingerprint.lock().await = Some(observed);
        Ok(true)
    }
}

/// Conexión viva junto al usuario del `Host` original.
struct StoredConnection {
    handle: Handle<FingerprintHandler>,
    username: String,
}

/// Conexión compartida: cada operación bloquea solo su propio handle, nunca el mapa.
type SharedConnection = Arc<Mutex<StoredConnection>>;

/// Transporte SSH con keepalive activo y connections indexadas por `ConnectionId`.
pub struct RusshTransport {
    config: Arc<Config>,
    connections: Mutex<HashMap<ConnectionId, SharedConnection>>,
    next_id: AtomicU64,
}

impl RusshTransport {
    /// Crea el transporte con keepalive de 30 s y tres reintentos.
    pub fn new() -> Self {
        let config = Config {
            keepalive_interval: Some(Duration::from_secs(30)),
            keepalive_max: 3,
            inactivity_timeout: None,
            ..Config::default()
        };
        Self {
            config: Arc::new(config),
            connections: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
        }
    }
}

impl Default for RusshTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl SshTransport for RusshTransport {
    async fn connect(&self, host: &Host) -> Result<Connected, TransportError> {
        let observed = Arc::new(Mutex::new(None));
        let handler = FingerprintHandler {
            fingerprint: Arc::clone(&observed),
        };
        let address = (host.hostname.as_str(), host.port.get());
        let handle = russh::client::connect(Arc::clone(&self.config), address, handler)
            .await
            .map_err(|_| TransportError::Connect)?;
        let raw = observed
            .lock()
            .await
            .take()
            .ok_or(TransportError::Connect)?;
        let fingerprint = HostKeyFingerprint::new(&raw).map_err(|_| TransportError::Connect)?;
        let id = ConnectionId::new(self.next_id.fetch_add(1, Ordering::Relaxed));
        let stored = StoredConnection {
            handle,
            username: host.username.as_str().to_owned(),
        };
        self.connections
            .lock()
            .await
            .insert(id, Arc::new(Mutex::new(stored)));
        Ok(Connected { id, fingerprint })
    }

    async fn authenticate(
        &self,
        conn: ConnectionId,
        credential: Credential,
    ) -> Result<(), TransportError> {
        let connection = {
            let connections = self.connections.lock().await;
            Arc::clone(connections.get(&conn).ok_or(TransportError::NotConnected)?)
        };
        let mut stored = connection.lock().await;
        let username = stored.username.clone();
        let result = match credential {
            Credential::Password(password) => {
                stored
                    .handle
                    .authenticate_password(&username, password.expose())
                    .await
            }
            Credential::PublicKey(key) => {
                let decoded = decode_key(key).await?;
                let hash_alg = stored
                    .handle
                    .best_supported_rsa_hash()
                    .await
                    .map_err(map_auth_error)?
                    .flatten();
                let method = PrivateKeyWithHashAlg::new(Arc::new(decoded), hash_alg);
                stored
                    .handle
                    .authenticate_publickey(&username, method)
                    .await
            }
        };
        match result {
            Ok(AuthResult::Success) => Ok(()),
            Ok(AuthResult::Failure { .. }) => Err(TransportError::AuthenticationRejected),
            Err(error) => Err(map_auth_error(error)),
        }
    }

    async fn open_pty(
        &self,
        _conn: ConnectionId,
        _size: PtySize,
        _sink: Box<dyn PtySink>,
    ) -> Result<ChannelId, TransportError> {
        Err(TransportError::NotConnected)
    }

    async fn write(&self, _channel: ChannelId, _bytes: &[u8]) -> Result<(), TransportError> {
        Err(TransportError::NotConnected)
    }

    async fn resize(&self, _channel: ChannelId, _size: PtySize) -> Result<(), TransportError> {
        Err(TransportError::NotConnected)
    }

    async fn close(&self, conn: ConnectionId) -> Result<(), TransportError> {
        let connection = {
            let mut connections = self.connections.lock().await;
            connections
                .remove(&conn)
                .ok_or(TransportError::NotConnected)?
        };
        let stored = connection.lock().await;
        stored
            .handle
            .disconnect(Disconnect::ByApplication, "", "")
            .await
            .map_err(|_| TransportError::Close)
    }
}

async fn decode_key(key: PrivateKey) -> Result<russh::keys::PrivateKey, TransportError> {
    tokio::task::spawn_blocking(move || {
        let text =
            std::str::from_utf8(key.as_bytes()).map_err(|_| TransportError::Authentication)?;
        russh::keys::decode_secret_key(text, None).map_err(|_| TransportError::Authentication)
    })
    .await
    .map_err(|_| TransportError::Authentication)?
}

fn map_auth_error(error: RusshError) -> TransportError {
    match error {
        RusshError::IO(_) => TransportError::Io,
        _ => TransportError::Authentication,
    }
}
