//! Puertos del dominio y tipos de apoyo. Sin I/O.

use std::future::Future;

use zeroize::Zeroizing;

use crate::auth::KeyRef;
use crate::error::DomainError;
use crate::host::Host;
use crate::port_errors::{
    FsError, HostKeyError, KeyError, RepositoryError, SecretError, TransportError,
};
use crate::remote_path::RemotePath;
use crate::session::{Folder, FolderId, Session, SessionId};

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

impl PtySize {
    /// Crea un tamaño rechazando dimensiones cero.
    pub fn new(rows: u16, cols: u16) -> Result<Self, DomainError> {
        if rows == 0 || cols == 0 {
            return Err(DomainError::InvalidPtySize);
        }
        Ok(Self { rows, cols })
    }
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

/// Clave de un secreto de sesión; nunca identifica una passphrase de llave (ADR-004).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SecretKey {
    pub session: SessionId,
}

/// Huella de la llave del host, con formato `SHA256:<base64>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostKeyFingerprint(String);

impl HostKeyFingerprint {
    /// Crea una huella rechazando la cadena vacía.
    pub fn new(value: &str) -> Result<Self, HostKeyError> {
        if value.is_empty() {
            return Err(HostKeyError::InvalidFingerprint);
        }
        Ok(Self(value.to_owned()))
    }

    /// Devuelve la huella como cadena.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Resultado del handshake: la conexión existe pero aún NO está autenticada.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Connected {
    pub id: ConnectionId,
    pub fingerprint: HostKeyFingerprint,
}

/// Resultado de comprobar la llave del host contra la lista de confianza.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostKeyStatus {
    Known,
    Unknown,
    Changed,
}

/// Receptor de la salida del PTY. Se invoca desde el adaptador; no debe bloquear.
#[cfg_attr(any(test, feature = "mocks"), mockall::automock)]
pub trait PtySink: Send + 'static {
    fn data(&mut self, bytes: &[u8]);
    fn closed(&mut self, exit_status: Option<u32>);
}

/// Conexión SSH y canales PTY.
#[cfg_attr(any(test, feature = "mocks"), mockall::automock)]
pub trait SshTransport: Send + Sync {
    fn connect(
        &self,
        host: &Host,
    ) -> impl Future<Output = Result<Connected, TransportError>> + Send;
    fn authenticate(
        &self,
        conn: ConnectionId,
        credential: Credential,
    ) -> impl Future<Output = Result<(), TransportError>> + Send;
    fn open_pty(
        &self,
        conn: ConnectionId,
        size: PtySize,
        sink: Box<dyn PtySink>,
    ) -> impl Future<Output = Result<ChannelId, TransportError>> + Send;
    fn write(
        &self,
        channel: ChannelId,
        bytes: &[u8],
    ) -> impl Future<Output = Result<(), TransportError>> + Send;
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

/// Carga de llaves privadas desde el sistema de archivos local.
#[cfg_attr(any(test, feature = "mocks"), mockall::automock)]
pub trait KeyLoader: Send + Sync {
    fn load<'a>(
        &self,
        key: &'a KeyRef,
        passphrase: Option<&'a SecretString>,
    ) -> impl Future<Output = Result<PrivateKey, KeyError>> + Send;
}

/// Almacén de archivos de llave en el directorio privado de la app.
#[cfg_attr(any(test, feature = "mocks"), mockall::automock)]
pub trait KeyFileStore: Send + Sync {
    /// Copia la llave al directorio privado de la app y devuelve la nueva referencia.
    fn import(&self, source: &KeyRef) -> impl Future<Output = Result<KeyRef, KeyError>> + Send;
}

/// Almacén de secretos de sesión; no expone operaciones para passphrases de llave (ADR-004).
#[cfg_attr(any(test, feature = "mocks"), mockall::automock)]
pub trait SecretStore: Send + Sync {
    fn get(
        &self,
        key: &SecretKey,
    ) -> impl Future<Output = Result<Option<SecretString>, SecretError>> + Send;
    /// Reemplaza el secreto existente.
    fn set(
        &self,
        key: &SecretKey,
        value: &SecretString,
    ) -> impl Future<Output = Result<(), SecretError>> + Send;
    /// Borra el secreto; no falla si ya estaba ausente.
    fn delete(&self, key: &SecretKey) -> impl Future<Output = Result<(), SecretError>> + Send;
}

/// Verificación de la llave del host contra la lista de confianza.
#[cfg_attr(any(test, feature = "mocks"), mockall::automock)]
pub trait HostKeyVerifier: Send + Sync {
    fn verify(
        &self,
        host: &Host,
        fingerprint: &HostKeyFingerprint,
    ) -> impl Future<Output = Result<HostKeyStatus, HostKeyError>> + Send;
    /// Confía explícitamente en la huella (TOFU).
    fn trust(
        &self,
        host: &Host,
        fingerprint: &HostKeyFingerprint,
    ) -> impl Future<Output = Result<(), HostKeyError>> + Send;
}

/// Persistencia de sesiones y carpetas del gestor.
#[cfg_attr(any(test, feature = "mocks"), mockall::automock)]
pub trait SessionRepository: Send + Sync {
    fn next_session_id(&self) -> impl Future<Output = Result<SessionId, RepositoryError>> + Send;
    fn next_folder_id(&self) -> impl Future<Output = Result<FolderId, RepositoryError>> + Send;
    fn list_sessions(&self) -> impl Future<Output = Result<Vec<Session>, RepositoryError>> + Send;
    fn get_session(
        &self,
        id: SessionId,
    ) -> impl Future<Output = Result<Option<Session>, RepositoryError>> + Send;
    /// Inserta o actualiza la sesión.
    fn save_session(
        &self,
        session: &Session,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;
    fn delete_session(
        &self,
        id: SessionId,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;
    fn list_folders(&self) -> impl Future<Output = Result<Vec<Folder>, RepositoryError>> + Send;
    fn save_folder(
        &self,
        folder: &Folder,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;
    fn delete_folder(
        &self,
        id: FolderId,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;
}

/// Parser incremental de OSC 7 que sigue el directorio de trabajo remoto.
#[cfg_attr(any(test, feature = "mocks"), mockall::automock)]
pub trait CwdTracker: Send {
    fn feed(&mut self, bytes: &[u8]) -> Option<RemotePath>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::DomainError;
    use crate::host::{Hostname, Port, Username};
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
    fn should_expose_fingerprint_before_authentication_when_connecting_a_host() {
        let mut mock = MockSshTransport::new();
        mock.expect_connect().once().returning(|_| {
            Box::pin(async {
                Ok(Connected {
                    id: ConnectionId::new(1),
                    fingerprint: HostKeyFingerprint::new("SHA256:abc").unwrap(),
                })
            })
        });
        let connected = block_on(mock.connect(&host())).unwrap();
        assert_eq!(connected.id, ConnectionId::new(1));
        assert_eq!(connected.fingerprint.as_str(), "SHA256:abc");
    }

    #[test]
    fn should_report_rejected_credentials_when_authenticating_a_connection() {
        let mut mock = MockSshTransport::new();
        mock.expect_authenticate()
            .once()
            .returning(|_, _| Box::pin(async { Err(TransportError::AuthenticationRejected) }));
        let result = block_on(mock.authenticate(
            ConnectionId::new(1),
            Credential::Password(SecretString::new("pw")),
        ));
        assert!(matches!(
            result,
            Err(TransportError::AuthenticationRejected)
        ));
    }

    #[test]
    fn should_receive_pty_data_when_mock_invokes_the_sink() {
        let mut sink = MockPtySink::new();
        sink.expect_data()
            .once()
            .withf(|bytes| *bytes == *b"ok")
            .returning(|_| ());
        let mut mock = MockSshTransport::new();
        mock.expect_open_pty().once().returning(|_, _, mut sink| {
            sink.data(b"ok");
            Box::pin(async { Ok(ChannelId::new(2)) })
        });
        let channel = block_on(mock.open_pty(
            ConnectionId::new(1),
            PtySize { rows: 24, cols: 80 },
            Box::new(sink),
        ))
        .unwrap();
        assert_eq!(channel, ChannelId::new(2));
    }

    #[test]
    fn should_receive_closed_status_when_mock_invokes_the_sink() {
        let mut sink = MockPtySink::new();
        sink.expect_closed()
            .once()
            .withf(|status| *status == Some(0))
            .returning(|_| ());
        let mut mock = MockSshTransport::new();
        mock.expect_open_pty().once().returning(|_, _, mut sink| {
            sink.closed(Some(0));
            Box::pin(async { Ok(ChannelId::new(3)) })
        });
        assert!(
            block_on(mock.open_pty(
                ConnectionId::new(1),
                PtySize { rows: 24, cols: 80 },
                Box::new(sink),
            ))
            .is_ok()
        );
    }

    #[test]
    fn should_mock_ssh_transport_when_writing_pty_input() {
        let mut mock = MockSshTransport::new();
        mock.expect_write().once().returning(|channel, bytes| {
            assert_eq!(channel, ChannelId::new(4));
            assert_eq!(bytes, b"ls\n");
            Box::pin(async { Ok(()) })
        });
        assert!(block_on(mock.write(ChannelId::new(4), b"ls\n")).is_ok());
    }

    #[test]
    fn should_report_unknown_connection_when_resizing_a_pty() {
        let mut mock = MockSshTransport::new();
        mock.expect_resize()
            .once()
            .returning(|_, _| Box::pin(async { Err(TransportError::NotConnected) }));
        let result = block_on(mock.resize(ChannelId::new(5), PtySize { rows: 24, cols: 80 }));
        assert!(matches!(result, Err(TransportError::NotConnected)));
    }

    #[test]
    fn should_reject_zero_dimensions_when_creating_pty_size() {
        assert_eq!(PtySize::new(0, 80), Err(DomainError::InvalidPtySize));
        assert_eq!(PtySize::new(24, 0), Err(DomainError::InvalidPtySize));
    }

    #[test]
    fn should_accept_positive_dimensions_when_creating_pty_size() {
        assert_eq!(PtySize::new(24, 80), Ok(PtySize { rows: 24, cols: 80 }));
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
    fn should_share_cancellation_state_when_cloning_cancel_token() {
        let token = CancelToken::default();
        let clone = token.clone();
        assert!(!token.is_cancelled());
        clone.cancel();
        assert!(token.is_cancelled());
        assert!(clone.is_cancelled());
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
        assert_eq!(
            block_on(mock.next_session_id()).unwrap(),
            SessionId::new(42)
        );
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
