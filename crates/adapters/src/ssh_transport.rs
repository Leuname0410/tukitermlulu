//! Adaptador SSH basado en `russh`: handshake, huella del host y autenticación.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use russh::client::{AuthResult, Config, Handle, Msg};
use russh::keys::{HashAlg, PrivateKeyWithHashAlg, PublicKeyOrCertificate};
use russh::{ChannelMsg, ChannelWriteHalf};
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

/// Escritor de un PTY junto a la conexión que lo originó.
struct PtyChannel {
    connection: ConnectionId,
    writer: Arc<ChannelWriteHalf<Msg>>,
}

/// Canales PTY compartidos con la tarea lectora; nunca se retiene el lock durante I/O.
type SharedChannels = Arc<Mutex<HashMap<ChannelId, PtyChannel>>>;

/// Estado interno compartido: todos los clones ven las mismas conexiones y canales.
struct Inner {
    config: Arc<Config>,
    connect_timeout: Duration,
    connections: Mutex<HashMap<ConnectionId, SharedConnection>>,
    channels: SharedChannels,
    next_id: AtomicU64,
    next_channel_id: AtomicU64,
}

/// Transporte SSH con keepalive activo y connections indexadas por `ConnectionId`.
#[derive(Clone)]
pub struct RusshTransport {
    inner: Arc<Inner>,
}

impl RusshTransport {
    /// Crea el transporte con 20 s de timeout de conexión y keepalive de 30 s.
    pub fn new() -> Self {
        Self::with_connect_timeout(Duration::from_secs(20))
    }

    /// Crea el transporte con un timeout de handshake personalizado.
    pub fn with_connect_timeout(connect_timeout: Duration) -> Self {
        let config = Config {
            keepalive_interval: Some(Duration::from_secs(30)),
            keepalive_max: 3,
            inactivity_timeout: None,
            ..Config::default()
        };
        Self {
            inner: Arc::new(Inner {
                config: Arc::new(config),
                connect_timeout,
                connections: Mutex::new(HashMap::new()),
                channels: Arc::new(Mutex::new(HashMap::new())),
                next_id: AtomicU64::new(1),
                next_channel_id: AtomicU64::new(1),
            }),
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
        let handle = tokio::time::timeout(
            self.inner.connect_timeout,
            russh::client::connect(Arc::clone(&self.inner.config), address, handler),
        )
        .await
        .map_err(|_| TransportError::Connect)?
        .map_err(|_| TransportError::Connect)?;
        let raw = observed
            .lock()
            .await
            .take()
            .ok_or(TransportError::Connect)?;
        let fingerprint = HostKeyFingerprint::new(&raw).map_err(|_| TransportError::Connect)?;
        let id = ConnectionId::new(self.inner.next_id.fetch_add(1, Ordering::Relaxed));
        let stored = StoredConnection {
            handle,
            username: host.username.as_str().to_owned(),
        };
        self.inner
            .connections
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
            let connections = self.inner.connections.lock().await;
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
        conn: ConnectionId,
        size: PtySize,
        sink: Box<dyn PtySink>,
    ) -> Result<ChannelId, TransportError> {
        let channel = {
            let connection = {
                let connections = self.inner.connections.lock().await;
                Arc::clone(connections.get(&conn).ok_or(TransportError::NotConnected)?)
            };
            let stored = connection.lock().await;
            stored
                .handle
                .channel_open_session()
                .await
                .map_err(|_| TransportError::OpenPty)?
        };
        channel
            .request_pty(
                false,
                "xterm-256color",
                u32::from(size.cols),
                u32::from(size.rows),
                0,
                0,
                &[],
            )
            .await
            .map_err(|_| TransportError::OpenPty)?;
        channel
            .request_shell(false)
            .await
            .map_err(|_| TransportError::OpenPty)?;

        let (mut reader, writer) = channel.split();
        let channel_id = ChannelId::new(self.inner.next_channel_id.fetch_add(1, Ordering::Relaxed));
        let channels = Arc::clone(&self.inner.channels);
        channels.lock().await.insert(
            channel_id,
            PtyChannel {
                connection: conn,
                writer: Arc::new(writer),
            },
        );

        let mut sink = sink;
        tokio::spawn(async move {
            let mut status = None;
            let mut eof = false;
            loop {
                let msg = if eof {
                    // OpenSSH envía EOF antes del exit-status: esperamos un instante para
                    // capturarlo sin bloquear el runtime ni retener locks del transporte.
                    match tokio::time::timeout(Duration::from_secs(1), reader.wait()).await {
                        Ok(Some(msg)) => msg,
                        Ok(None) | Err(_) => break,
                    }
                } else {
                    match reader.wait().await {
                        Some(msg) => msg,
                        None => break,
                    }
                };
                match msg {
                    ChannelMsg::Data { data } => {
                        // `PtySink` es síncrono y debe ser barato: esta tarea nunca hace I/O
                        // ni retiene locks del transporte mientras entrega los bytes.
                        sink.data(&data);
                    }
                    ChannelMsg::ExitStatus { exit_status } => status = Some(exit_status),
                    ChannelMsg::Eof => eof = true,
                    ChannelMsg::Close => break,
                    _ => {}
                }
            }
            channels.lock().await.remove(&channel_id);
            sink.closed(status);
        });

        Ok(channel_id)
    }

    async fn write(&self, channel: ChannelId, bytes: &[u8]) -> Result<(), TransportError> {
        let writer = {
            let channels = self.inner.channels.lock().await;
            Arc::clone(
                &channels
                    .get(&channel)
                    .ok_or(TransportError::NotConnected)?
                    .writer,
            )
        };
        writer
            .data_bytes(bytes.to_vec())
            .await
            .map_err(|_| TransportError::Io)
    }

    async fn resize(&self, channel: ChannelId, size: PtySize) -> Result<(), TransportError> {
        let writer = {
            let channels = self.inner.channels.lock().await;
            Arc::clone(
                &channels
                    .get(&channel)
                    .ok_or(TransportError::NotConnected)?
                    .writer,
            )
        };
        writer
            .window_change(u32::from(size.cols), u32::from(size.rows), 0, 0)
            .await
            .map_err(|_| TransportError::Resize)
    }

    async fn close(&self, conn: ConnectionId) -> Result<(), TransportError> {
        let connection = {
            let mut connections = self.inner.connections.lock().await;
            connections
                .remove(&conn)
                .ok_or(TransportError::NotConnected)?
        };
        let writers = {
            let mut channels = self.inner.channels.lock().await;
            let ids: Vec<ChannelId> = channels
                .iter()
                .filter(|(_, pty)| pty.connection == conn)
                .map(|(id, _)| *id)
                .collect();
            let mut writers = Vec::with_capacity(ids.len());
            for id in ids {
                if let Some(pty) = channels.remove(&id) {
                    writers.push(pty.writer);
                }
            }
            writers
        };
        for writer in writers {
            let _ = writer.close().await;
        }
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
