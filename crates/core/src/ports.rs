//! Puertos del dominio y tipos de apoyo. Sin I/O.

use std::future::Future;

use zeroize::Zeroizing;

use crate::host::Host;
use crate::port_errors::{FsError, TransportError};
use crate::remote_path::RemotePath;

/// Identificador opaco de una conexión SSH.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConnectionId(u64);

impl ConnectionId {
    /// Crea un identificador de conexión.
    pub fn new(v: u64) -> Self {
        Self(v)
    }
    /// Devuelve el valor numérico del identificador.
    pub fn get(self) -> u64 {
        self.0
    }
}

/// Identificador opaco de un canal PTY.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChannelId(u64);

impl ChannelId {
    /// Crea un identificador de canal.
    pub fn new(v: u64) -> Self {
        Self(v)
    }
    /// Devuelve el valor numérico del identificador.
    pub fn get(self) -> u64 {
        self.0
    }
}

/// Tamaño de la terminal remota.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PtySize {
    pub rows: u16,
    pub cols: u16,
}

/// Secreto de texto retenido solo en memoria.
pub struct SecretString(Zeroizing<String>);

impl SecretString {
    /// Crea un secreto a partir de una cadena.
    pub fn new(value: impl Into<String>) -> Self {
        Self(Zeroizing::new(value.into()))
    }
    /// Expone el contenido únicamente a adaptadores de confianza.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl From<&str> for SecretString {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl std::fmt::Debug for SecretString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SecretString([REDACTED])")
    }
}

/// Llave privada cargada, borrada de memoria al caer.
pub struct PrivateKey(Zeroizing<Vec<u8>>);

impl PrivateKey {
    /// Crea una llave a partir de bytes.
    pub fn new(bytes: impl Into<Vec<u8>>) -> Self {
        Self(Zeroizing::new(bytes.into()))
    }
    /// Devuelve los bytes de la llave para el transporte.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl std::fmt::Debug for PrivateKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PrivateKey([REDACTED])")
    }
}

/// Credencial de autenticación; la contraseña viaja envuelta.
#[derive(Debug)]
pub enum Credential {
    /// Contraseña de la sesión.
    Password(SecretString),
    /// Llave privada ya cargada por `KeyLoader`.
    PublicKey(PrivateKey),
}

/// Entrada de un listado remoto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    pub path: RemotePath,
    pub is_dir: bool,
    pub size: u64,
}

/// Atributos de un archivo remoto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileStat {
    pub size: u64,
    pub is_dir: bool,
}

/// Conexión SSH y canales PTY.
#[cfg_attr(any(test, feature = "mocks"), mockall::automock)]
pub trait SshTransport: Send + Sync {
    fn connect(
        &self,
        host: &Host,
    ) -> impl Future<Output = Result<ConnectionId, TransportError>> + Send;
    fn authenticate(
        &self,
        conn: ConnectionId,
        credential: Credential,
    ) -> impl Future<Output = Result<(), TransportError>> + Send;
    fn open_pty(
        &self,
        conn: ConnectionId,
        size: PtySize,
    ) -> impl Future<Output = Result<ChannelId, TransportError>> + Send;
    fn resize(
        &self,
        channel: ChannelId,
        size: PtySize,
    ) -> impl Future<Output = Result<(), TransportError>> + Send;
    fn close(&self, conn: ConnectionId) -> impl Future<Output = Result<(), TransportError>> + Send;
}

/// Operaciones básicas sobre el sistema de archivos remoto.
#[cfg_attr(any(test, feature = "mocks"), mockall::automock)]
pub trait RemoteFileSystem: Send + Sync {
    fn list(
        &self,
        conn: ConnectionId,
        path: &RemotePath,
    ) -> impl Future<Output = Result<Vec<FileEntry>, FsError>> + Send;
    fn stat(
        &self,
        conn: ConnectionId,
        path: &RemotePath,
    ) -> impl Future<Output = Result<FileStat, FsError>> + Send;
    fn rename(
        &self,
        conn: ConnectionId,
        from: &RemotePath,
        to: &RemotePath,
    ) -> impl Future<Output = Result<(), FsError>> + Send;
    fn remove(
        &self,
        conn: ConnectionId,
        path: &RemotePath,
    ) -> impl Future<Output = Result<(), FsError>> + Send;
    fn mkdir(
        &self,
        conn: ConnectionId,
        path: &RemotePath,
    ) -> impl Future<Output = Result<(), FsError>> + Send;
}

/// Parser incremental de OSC 7 que sigue el directorio de trabajo remoto.
#[cfg_attr(any(test, feature = "mocks"), mockall::automock)]
pub trait CwdTracker: Send {
    fn feed(&mut self, bytes: &[u8]) -> Option<RemotePath>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::KeyRef;
    use crate::host::{Hostname, Port, Username};
    use crate::port_errors::HostKeyError;
    use crate::session::SessionId;
    use std::future::Future;
    use std::task::{Context, Poll, Waker};

    fn block_on<F: Future>(future: F) -> F::Output {
        let mut context = Context::from_waker(Waker::noop());
        let mut future = std::pin::pin!(future);
        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => output,
            Poll::Pending => panic!("future pending"),
        }
    }

    fn path() -> RemotePath {
        RemotePath::new("/tmp").unwrap()
    }

    fn host() -> Host {
        Host {
            hostname: Hostname::new("example.com").unwrap(),
            port: Port::default(),
            username: Username::new("alice").unwrap(),
        }
    }

    #[test]
    fn should_mock_ssh_transport_when_closing_a_connection() {
        let mut mock = MockSshTransport::new();
        mock.expect_close()
            .once()
            .returning(|_| Box::pin(async { Ok(()) }));
        assert!(block_on(mock.close(ConnectionId::new(1))).is_ok());
    }

    #[test]
    fn should_mock_remote_file_system_when_stating_a_path() {
        let mut mock = MockRemoteFileSystem::new();
        mock.expect_stat().once().returning(|_, _| {
            Box::pin(async {
                Ok(FileStat {
                    size: 0,
                    is_dir: false,
                })
            })
        });
        let stat = block_on(mock.stat(ConnectionId::new(1), &path())).unwrap();
        assert_eq!(stat.size, 0);
    }

    #[test]
    fn should_mock_cwd_tracker_when_feeding_osc7_bytes() {
        let mut mock = MockCwdTracker::new();
        mock.expect_feed()
            .once()
            .returning(|_| Some(RemotePath::new("/srv").unwrap()));
        assert_eq!(mock.feed(b"x"), Some(RemotePath::new("/srv").unwrap()));
    }

    #[test]
    fn should_redact_secret_string_when_debug_formatting() {
        let debug = format!("{:?}", SecretString::from("topsecret"));
        assert!(!debug.contains("topsecret"));
        assert!(debug.contains("[REDACTED]"));
    }

    #[test]
    fn should_mock_key_loader_when_loading_a_key() {
        let mut mock = MockKeyLoader::new();
        mock.expect_load()
            .once()
            .returning(|_, _| Box::pin(async { Ok(PrivateKey::new(vec![1, 2, 3])) }));
        let key = KeyRef::new("~/.ssh/id_ed25519").unwrap();
        let loaded = block_on(mock.load(&key, None)).unwrap();
        assert_eq!(loaded.as_bytes(), &[1, 2, 3]);
    }

    #[test]
    fn should_mock_secret_store_when_getting_a_session_secret() {
        let mut mock = MockSecretStore::new();
        mock.expect_get()
            .once()
            .returning(|_| Box::pin(async { Ok(Some(SecretString::new("stored"))) }));
        let key = SecretKey {
            session: SessionId::new(7),
        };
        let secret = block_on(mock.get(&key)).unwrap().unwrap();
        assert_eq!(secret.expose(), "stored");
    }

    #[test]
    fn should_mock_host_key_verifier_when_verifying_a_fingerprint() {
        let mut mock = MockHostKeyVerifier::new();
        mock.expect_verify()
            .once()
            .returning(|_, _| Box::pin(async { Ok(HostKeyStatus::Known) }));
        let fingerprint = HostKeyFingerprint::new("SHA256:abc").unwrap();
        let status = block_on(mock.verify(&host(), &fingerprint)).unwrap();
        assert_eq!(status, HostKeyStatus::Known);
    }

    #[test]
    fn should_mock_session_repository_when_getting_the_next_session_id() {
        let mut mock = MockSessionRepository::new();
        mock.expect_next_session_id()
            .once()
            .returning(|| Box::pin(async { Ok(SessionId::new(42)) }));
        assert_eq!(block_on(mock.next_session_id()).unwrap(), SessionId::new(42));
    }

    #[test]
    fn should_reject_empty_fingerprint_when_creating_host_key_fingerprint() {
        assert_eq!(
            HostKeyFingerprint::new(""),
            Err(HostKeyError::InvalidFingerprint)
        );
    }

    #[test]
    fn should_expose_fingerprint_when_creating_host_key_fingerprint() {
        let fingerprint = HostKeyFingerprint::new("SHA256:abc").unwrap();
        assert_eq!(fingerprint.as_str(), "SHA256:abc");
    }
}
