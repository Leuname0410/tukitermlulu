//! Puente IPC de la terminal: DTOs, comandos y canal binario de salida.

use std::fmt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri::State;
use tauri::ipc::{Channel, InvokeBody, InvokeResponseBody, Request};
use tokio::sync::Notify;
use tuki_adapters::cwd_tracker::Osc7Tracker;
use tuki_adapters::echo_filter::EchoFilter;
use tuki_adapters::key_loader::RusshKeyLoader;
use tuki_adapters::known_hosts::FileHostKeyVerifier;
use tuki_adapters::secret_store::KeyringSecretStore;
use tuki_adapters::session_repository::SqliteSessionRepository;
use tuki_adapters::sftp::RusshFileSystem;
use tuki_adapters::ssh_transport::RusshTransport;
use tuki_application::connect_session::{
    ConnectError, ConnectOutcome, ConnectRequest, ConnectSession,
};
use tuki_application::connect_tab::{ConnectTab, TabId, TabPassphrases};
use tuki_application::session_errors::SessionError;
use tuki_application::transfer_queue::TransferQueue;
use tuki_core::auth::{AuthMethod, KeyRef};
use tuki_core::host::{Host, Hostname, Port, Username};
use tuki_core::port_errors::{KeyError, RepositoryError, TransportError};
use tuki_core::ports::{
    ChannelId, ConnectionId, CwdTracker, HostKeyFingerprint, PtySink, PtySize, SecretString,
    SessionRepository, SshTransport,
};
use tuki_core::remote_path::RemotePath;
use tuki_core::session::{Session, SessionId};
use zeroize::Zeroizing;

use crate::shell_hook::{HookTiming, QuietClock, integrate};
use crate::transfers::emit_transfers;

/// Estado compartido por los comandos: transporte, known_hosts, passphrases y repositorio.
pub struct AppState {
    transport: RusshTransport,
    known_hosts_path: PathBuf,
    passphrases: TabPassphrases,
    pub(crate) repository: SqliteSessionRepository,
    pub(crate) fs: RusshFileSystem,
    pub(crate) transfers: Arc<Mutex<TransferQueue>>,
    pub(crate) notify: Arc<Notify>,
}

impl AppState {
    /// Crea el estado de la composition root.
    pub(crate) fn new(
        transport: RusshTransport,
        known_hosts_path: PathBuf,
        repository: SqliteSessionRepository,
    ) -> Self {
        Self {
            fs: RusshFileSystem::new(transport.clone()),
            transport,
            known_hosts_path,
            passphrases: TabPassphrases::new(),
            repository,
            transfers: Arc::new(Mutex::new(TransferQueue::default())),
            notify: Arc::new(Notify::new()),
        }
    }
}

/// Construye el almacén de secretos del sistema en un único punto de la composición.
pub(crate) fn secret_store() -> KeyringSecretStore {
    KeyringSecretStore::new("tukitermlulu")
}

/// Método de autenticación descrito por la UI; nunca incluye secretos.
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AuthArgs {
    Password { remember: bool },
    Key { path: String, has_passphrase: bool },
}

/// Argumentos del comando `connect`. Los secretos se descartan al construir el dominio.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectArgs {
    session_id: Option<u64>,
    host: String,
    port: u16,
    username: String,
    auth: AuthArgs,
    password: Option<Zeroizing<String>>,
    passphrase: Option<Zeroizing<String>>,
    approved_fingerprint: Option<String>,
    rows: u16,
    cols: u16,
}

impl fmt::Debug for ConnectArgs {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ConnectArgs")
            .field("session_id", &self.session_id)
            .field("host", &self.host)
            .field("port", &self.port)
            .field("username", &self.username)
            .field("auth", &self.auth)
            .field("password", &"[REDACTED]")
            .field("passphrase", &"[REDACTED]")
            .field("approved_fingerprint", &self.approved_fingerprint)
            .field("rows", &self.rows)
            .field("cols", &self.cols)
            .finish()
    }
}

/// Resultado de la conexión, con los identificadores que usará la UI.
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ConnectResponse {
    Connected { connection_id: u64, channel_id: u64 },
    NeedsHostKeyApproval { fingerprint: String },
    NeedsPassword,
    NeedsPassphrase,
}

/// Error de comando serializable: mensajes fijos del dominio, sin secretos.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CommandError {
    pub(crate) kind: &'static str,
    message: String,
    fingerprint: Option<String>,
}

/// Aviso de cierre del PTY enviado por el canal de cierre.
#[derive(Deserialize, Serialize, Clone)]
pub struct TerminalClosed {
    exit_status: Option<u32>,
}

/// Aviso de cambio de directorio remoto detectado en la salida del PTY.
#[derive(Serialize, Clone)]
pub struct CwdChanged {
    path: String,
}

/// Entrega la salida del PTY como bytes raw, notifica su cierre y el cwd.
pub struct ChannelSink {
    output: Arc<Mutex<Channel<InvokeResponseBody>>>,
    closed: Channel<TerminalClosed>,
    tracker: Osc7Tracker,
    last_cwd: Option<RemotePath>,
    cwd: Channel<CwdChanged>,
    filter: Arc<Mutex<EchoFilter>>,
    clock: QuietClock,
}

impl ChannelSink {
    /// Compone el sink con el filtro y el reloj compartidos con la integración.
    fn new(
        output: Arc<Mutex<Channel<InvokeResponseBody>>>,
        closed: Channel<TerminalClosed>,
        cwd: Channel<CwdChanged>,
        filter: Arc<Mutex<EchoFilter>>,
        clock: QuietClock,
    ) -> Self {
        Self {
            output,
            closed,
            tracker: Osc7Tracker::new(),
            last_cwd: None,
            cwd,
            filter,
            clock,
        }
    }
}

impl PtySink for ChannelSink {
    fn data(&mut self, bytes: &[u8]) {
        self.clock.mark(Instant::now());
        {
            // Mismo orden de locks que la integración: filtro -> output.
            let mut filter = self
                .filter
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let visible = filter.push(bytes);
            if !visible.is_empty() {
                let _ = self
                    .output
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .send(InvokeResponseBody::Raw(visible));
            }
        }
        if let Some(path) = self
            .tracker
            .feed(bytes)
            .filter(|path| self.last_cwd.as_ref() != Some(path))
        {
            let _ = self.cwd.send(CwdChanged {
                path: path.as_str().to_owned(),
            });
            self.last_cwd = Some(path);
        }
    }

    fn closed(&mut self, exit_status: Option<u32>) {
        let _ = self.closed.send(TerminalClosed { exit_status });
    }
}

impl From<ConnectOutcome> for ConnectResponse {
    fn from(outcome: ConnectOutcome) -> Self {
        match outcome {
            ConnectOutcome::Connected {
                connection,
                channel,
            } => Self::Connected {
                connection_id: connection.get(),
                channel_id: channel.get(),
            },
            ConnectOutcome::NeedsHostKeyApproval { fingerprint } => Self::NeedsHostKeyApproval {
                fingerprint: fingerprint.as_str().to_owned(),
            },
            ConnectOutcome::NeedsPassword => Self::NeedsPassword,
            ConnectOutcome::NeedsPassphrase => Self::NeedsPassphrase,
        }
    }
}

impl CommandError {
    /// Construye un error con el mensaje accionable del dominio.
    pub(crate) fn new(kind: &'static str, message: impl fmt::Display) -> Self {
        Self {
            kind,
            message: message.to_string(),
            fingerprint: None,
        }
    }

    /// Devuelve la categoría del error para los tests de mapeo.
    #[cfg(test)]
    pub(crate) fn kind(&self) -> &'static str {
        self.kind
    }

    pub(crate) fn invalid_input(error: impl fmt::Display) -> Self {
        Self::new("invalid_input", error)
    }

    pub(crate) fn not_found(error: impl fmt::Display) -> Self {
        Self::new("not_found", error)
    }

    /// Traduce los fallos del gestor de sesiones sin filtrar secretos.
    pub(crate) fn from_session(error: SessionError) -> Self {
        let message = error.to_string();
        let kind = match &error {
            SessionError::Domain(_) => "invalid_input",
            SessionError::Repository(RepositoryError::NotFound) => "not_found",
            SessionError::Repository(RepositoryError::Conflict) => "conflict",
            SessionError::Repository(RepositoryError::Io) | SessionError::Secret(_) => "io",
        };
        Self {
            kind,
            message,
            fingerprint: None,
        }
    }

    fn from_connect(error: ConnectError) -> Self {
        let message = error.to_string();
        match error {
            ConnectError::HostKeyChanged { fingerprint } => Self {
                kind: "host_key_changed",
                message,
                fingerprint: Some(fingerprint.as_str().to_owned()),
            },
            ConnectError::Transport(error) => Self::from_transport(error),
            ConnectError::Key(error) => Self::from_key(error),
            ConnectError::Secret(error) => Self::new("io", error),
            ConnectError::HostKey(error) => Self::new("io", error),
        }
    }

    fn from_transport(error: TransportError) -> Self {
        let kind = match &error {
            TransportError::AuthenticationRejected => "auth_rejected",
            TransportError::NotConnected => "not_connected",
            _ => "io",
        };
        Self::new(kind, error)
    }

    fn from_key(error: KeyError) -> Self {
        let kind = match &error {
            KeyError::WrongPassphrase => "wrong_passphrase",
            _ => "io",
        };
        Self::new(kind, error)
    }
}

type DomainParts = (
    Session,
    PtySize,
    Option<SecretString>,
    Option<SecretString>,
    Option<HostKeyFingerprint>,
);

/// Carga la sesión guardada cuando el cliente envía `sessionId`; ignora el resto de args.
async fn session_for_connect(
    repository: &SqliteSessionRepository,
    args: &ConnectArgs,
) -> Result<Option<Session>, CommandError> {
    let Some(id) = args.session_id else {
        return Ok(None);
    };
    let session = repository
        .get_session(SessionId::new(id))
        .await
        .map_err(|error| CommandError::from_session(error.into()))?;
    session
        .ok_or_else(|| CommandError::not_found("session not found"))
        .map(Some)
}

/// Valida los DTOs sin tocar la red y copia los secretos a tipos envueltos.
fn to_domain(args: &ConnectArgs, stored: Option<Session>) -> Result<DomainParts, CommandError> {
    let session = match stored {
        Some(session) => session,
        None => {
            let host = Host {
                hostname: Hostname::new(&args.host).map_err(CommandError::invalid_input)?,
                port: Port::new(args.port).map_err(CommandError::invalid_input)?,
                username: Username::new(&args.username).map_err(CommandError::invalid_input)?,
            };
            let auth = match &args.auth {
                AuthArgs::Password { remember } => AuthMethod::Password {
                    remember: *remember,
                },
                AuthArgs::Key {
                    path,
                    has_passphrase,
                } => AuthMethod::PublicKey {
                    key: KeyRef::new(path).map_err(CommandError::invalid_input)?,
                    has_passphrase: *has_passphrase,
                },
            };
            Session::new(SessionId::new(0), &args.host, host, auth, None)
                .map_err(CommandError::invalid_input)?
        }
    };
    let pty_size = PtySize::new(args.rows, args.cols).map_err(CommandError::invalid_input)?;
    let approved = args
        .approved_fingerprint
        .as_deref()
        .map(HostKeyFingerprint::new)
        .transpose()
        .map_err(CommandError::invalid_input)?;
    let password = args
        .password
        .as_ref()
        .map(|secret| SecretString::new(secret.as_str()));
    let passphrase = args
        .passphrase
        .as_ref()
        .map(|secret| SecretString::new(secret.as_str()));
    Ok((session, pty_size, password, passphrase, approved))
}

/// Conecta y abre un PTY; la salida viaja por `on_output`, el cierre por `on_closed` y el cwd por `on_cwd`.
#[tauri::command]
pub async fn connect(
    state: State<'_, AppState>,
    tab_id: u64,
    args: ConnectArgs,
    on_output: Channel<InvokeResponseBody>,
    on_closed: Channel<TerminalClosed>,
    on_cwd: Channel<CwdChanged>,
) -> Result<ConnectResponse, CommandError> {
    let stored = session_for_connect(&state.repository, &args).await?;
    let (session, pty_size, password, passphrase, approved_fingerprint) = to_domain(&args, stored)?;
    drop(args);
    let connect = ConnectSession::new(
        state.transport.clone(),
        FileHostKeyVerifier::new(state.known_hosts_path.clone()),
        RusshKeyLoader,
        secret_store(),
    );
    let use_case = ConnectTab::new(connect, &state.passphrases);
    let output = Arc::new(Mutex::new(on_output));
    let filter = Arc::new(Mutex::new(EchoFilter::new()));
    let clock = QuietClock::new(Instant::now());
    let request = ConnectRequest {
        session: &session,
        password: password.as_ref(),
        passphrase: passphrase.as_ref(),
        approved_fingerprint: approved_fingerprint.as_ref(),
        pty_size,
        sink: Box::new(ChannelSink::new(
            Arc::clone(&output),
            on_closed,
            on_cwd,
            Arc::clone(&filter),
            clock.clone(),
        )),
    };
    let outcome = use_case
        .execute(TabId::new(tab_id), request)
        .await
        .map_err(CommandError::from_connect)?;
    let response = ConnectResponse::from(outcome);
    let connected_ids = match &response {
        ConnectResponse::Connected {
            connection_id,
            channel_id,
        } => Some((*connection_id, *channel_id)),
        _ => None,
    };
    if session.shell_integration
        && let Some((connection_id, channel_id)) = connected_ids
    {
        let transport = Arc::new(state.transport.clone());
        let task_output = Arc::clone(&output);
        let task_filter = Arc::clone(&filter);
        let task_clock = clock.clone();
        tauri::async_runtime::spawn(async move {
            integrate(
                transport,
                ConnectionId::new(connection_id),
                ChannelId::new(channel_id),
                task_clock,
                task_filter,
                HookTiming::default(),
                |pending| {
                    let _ = task_output
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .send(InvokeResponseBody::Raw(pending));
                },
            )
            .await;
        });
    }
    Ok(response)
}

/// Olvida la passphrase retenida por la pestaña al cerrarla; no desconecta.
#[tauri::command]
pub fn close_tab(state: State<'_, AppState>, tab_id: u64) {
    state.passphrases.forget(TabId::new(tab_id));
}

/// Escribe el cuerpo raw de la petición en el PTY indicado por `x-channel-id`.
#[tauri::command]
pub async fn terminal_write(
    state: State<'_, AppState>,
    request: Request<'_>,
) -> Result<(), CommandError> {
    let channel_id = request
        .headers()
        .get("x-channel-id")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .ok_or_else(|| CommandError::invalid_input("missing or invalid x-channel-id"))?;
    let InvokeBody::Raw(bytes) = request.body() else {
        return Err(CommandError::invalid_input(
            "terminal input must be raw bytes",
        ));
    };
    state
        .transport
        .write(ChannelId::new(channel_id), bytes)
        .await
        .map_err(CommandError::from_transport)
}

/// Ajusta el tamaño de un PTY existente.
#[tauri::command]
pub async fn terminal_resize(
    state: State<'_, AppState>,
    channel_id: u64,
    rows: u16,
    cols: u16,
) -> Result<(), CommandError> {
    let size = PtySize::new(rows, cols).map_err(CommandError::invalid_input)?;
    state
        .transport
        .resize(ChannelId::new(channel_id), size)
        .await
        .map_err(CommandError::from_transport)
}

/// Cierra una conexión, sus PTY asociados y cancela sus transferencias.
#[tauri::command]
pub async fn disconnect(
    state: State<'_, AppState>,
    app: AppHandle,
    connection_id: u64,
) -> Result<(), CommandError> {
    {
        let mut transfers = state
            .transfers
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        transfers.cancel_connection(ConnectionId::new(connection_id));
        emit_transfers(&app, &transfers);
    }
    state
        .transport
        .close(ConnectionId::new(connection_id))
        .await
        .map_err(CommandError::from_transport)
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::sync::{Arc, Mutex};

    use tauri::ipc::{Channel, InvokeResponseBody};
    use tuki_adapters::session_repository::SqliteSessionRepository;
    use tuki_core::port_errors::{HostKeyError, KeyError, SecretError, TransportError};
    use tuki_core::ports::{ChannelId, ConnectionId, HostKeyFingerprint, PtySize};

    use super::*;

    type RecordedOutput = Arc<Mutex<Vec<InvokeResponseBody>>>;
    type RecordedPaths = Arc<Mutex<Vec<String>>>;

    fn noop_cwd() -> Channel<CwdChanged> {
        Channel::new(move |_body| Ok(()))
    }

    /// Crea un sink que captura la salida raw y las rutas emitidas por el canal de cwd.
    fn recording_sink() -> (ChannelSink, RecordedOutput, RecordedPaths) {
        let output_bytes = Arc::new(Mutex::new(Vec::new()));
        let output = {
            let seen = Arc::clone(&output_bytes);
            Channel::new(move |body| {
                seen.lock().unwrap().push(body);
                Ok(())
            })
        };
        let cwd_paths = Arc::new(Mutex::new(Vec::new()));
        let cwd = {
            let seen = Arc::clone(&cwd_paths);
            Channel::new(move |body| {
                if let InvokeResponseBody::Json(json) = body {
                    let message: serde_json::Value = serde_json::from_str(&json).unwrap();
                    if let Some(path) = message.get("path").and_then(|value| value.as_str()) {
                        seen.lock().unwrap().push(path.to_owned());
                    }
                }
                Ok(())
            })
        };
        let sink = ChannelSink::new(
            Arc::new(Mutex::new(output)),
            Channel::new(move |_body| Ok(())),
            cwd,
            Arc::new(Mutex::new(EchoFilter::new())),
            QuietClock::new(Instant::now()),
        );
        (sink, output_bytes, cwd_paths)
    }

    fn block_on<F: Future>(future: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(future)
    }

    fn args() -> ConnectArgs {
        ConnectArgs {
            session_id: None,
            host: "example.com".to_owned(),
            port: 22,
            username: "alice".to_owned(),
            auth: AuthArgs::Password { remember: false },
            password: Some(Zeroizing::new("pw-secret".to_owned())),
            passphrase: Some(Zeroizing::new("pass-secret".to_owned())),
            approved_fingerprint: None,
            rows: 24,
            cols: 80,
        }
    }

    #[test]
    fn should_deliver_raw_bytes_and_close_status_when_pty_writes() {
        let output_bytes = Arc::new(Mutex::new(Vec::new()));
        let output = {
            let seen = Arc::clone(&output_bytes);
            Channel::new(move |body| {
                seen.lock().unwrap().push(body);
                Ok(())
            })
        };
        let exit_status = Arc::new(Mutex::new(None));
        let closed = {
            let seen = Arc::clone(&exit_status);
            Channel::new(move |body| {
                if let InvokeResponseBody::Json(json) = body {
                    let message: TerminalClosed = serde_json::from_str(&json).unwrap();
                    *seen.lock().unwrap() = message.exit_status;
                }
                Ok(())
            })
        };
        let mut sink = ChannelSink::new(
            Arc::new(Mutex::new(output)),
            closed,
            noop_cwd(),
            Arc::new(Mutex::new(EchoFilter::new())),
            QuietClock::new(Instant::now()),
        );

        sink.data(&[1, 2, 3]);
        sink.closed(Some(0));

        let bodies = output_bytes.lock().unwrap();
        assert_eq!(bodies.len(), 1);
        match &bodies[0] {
            InvokeResponseBody::Raw(bytes) => assert_eq!(bytes.as_slice(), &[1, 2, 3]),
            other => panic!("cuerpo inesperado: {other:?}"),
        }
        assert_eq!(*exit_status.lock().unwrap(), Some(0));
    }

    #[test]
    fn should_forward_osc7_path_without_modifying_raw_output_when_pty_writes() {
        let chunk = b"prompt $ \x1b]7;file://host/tmp\x07ls\n";
        let (mut sink, output_bytes, cwd_paths) = recording_sink();

        sink.data(chunk);

        let bodies = output_bytes.lock().unwrap();
        assert_eq!(bodies.len(), 1);
        match &bodies[0] {
            InvokeResponseBody::Raw(bytes) => assert_eq!(bytes.as_slice(), chunk),
            other => panic!("cuerpo inesperado: {other:?}"),
        }
        assert_eq!(*cwd_paths.lock().unwrap(), vec!["/tmp".to_owned()]);
        assert_eq!(
            serde_json::to_string(&CwdChanged {
                path: "/tmp".to_owned()
            })
            .unwrap(),
            r#"{"path":"/tmp"}"#
        );
    }

    #[test]
    fn should_flush_kept_bytes_before_new_output_when_expiring_while_data_arrives() {
        use std::sync::mpsc;
        use std::time::Duration;

        use tuki_core::ports::{ExecOutput, MockSshTransport};

        let filter = Arc::new(Mutex::new(EchoFilter::new()));
        let output_bytes = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));
        let output = {
            let seen = Arc::clone(&output_bytes);
            let probe = Arc::clone(&filter);
            Arc::new(Mutex::new(Channel::new(move |body| {
                // Orden fijo de locks: filtro -> output, también al enviar.
                assert!(probe.try_lock().is_err());
                if let InvokeResponseBody::Raw(bytes) = body {
                    seen.lock().unwrap().push(bytes);
                }
                Ok(())
            })))
        };
        let clock = QuietClock::new(Instant::now());
        let mut sink = ChannelSink::new(
            Arc::clone(&output),
            Channel::new(move |_body| Ok(())),
            noop_cwd(),
            Arc::clone(&filter),
            clock.clone(),
        );

        // La shell ecoa la línea inyectada mientras el filtro está activo.
        let echo_filter = Arc::clone(&filter);
        let mut transport = MockSshTransport::new();
        transport.expect_exec().once().returning(|_, _| {
            Box::pin(async {
                Ok(ExecOutput {
                    stdout: b"::/bin/bash".to_vec(),
                    stderr: Vec::new(),
                    status: Some(0),
                })
            })
        });
        transport.expect_write().once().returning(move |_, _| {
            echo_filter.lock().unwrap().push(b"eco oculto");
            Box::pin(async { Ok(()) })
        });

        let (start_tx, start_rx) = mpsc::channel();
        let (attempt_tx, attempt_rx) = mpsc::channel();
        let data_thread = std::thread::spawn(move || {
            start_rx.recv().unwrap();
            attempt_tx.send(()).unwrap();
            sink.data(b"prompt$ ");
        });

        let flush_output = Arc::clone(&output);
        block_on(integrate(
            Arc::new(transport),
            ConnectionId::new(1),
            ChannelId::new(2),
            clock,
            filter,
            HookTiming {
                quiet: Duration::ZERO,
                max_wait: Duration::ZERO,
                expire: Duration::ZERO,
            },
            move |pending| {
                start_tx.send(()).unwrap();
                attempt_rx.recv_timeout(Duration::from_secs(1)).unwrap();
                let _ = flush_output
                    .lock()
                    .unwrap()
                    .send(InvokeResponseBody::Raw(pending));
            },
        ));
        data_thread.join().unwrap();

        let bodies = output_bytes.lock().unwrap();
        let visible: Vec<&[u8]> = bodies.iter().map(Vec::as_slice).collect();
        assert_eq!(
            visible,
            vec![b"eco oculto".as_slice(), b"prompt$ ".as_slice()]
        );
    }

    #[test]
    fn should_emit_cwd_only_when_the_path_changes() {
        let (mut sink, _, cwd_paths) = recording_sink();

        sink.data(b"\x1b]7;file:///tmp\x07");
        sink.data(b"cd . \x1b]7;file:///tmp\x07");
        sink.data(b"\x1b]7;file:///var\x07");

        assert_eq!(
            *cwd_paths.lock().unwrap(),
            vec!["/tmp".to_owned(), "/var".to_owned()]
        );
    }

    #[test]
    fn should_emit_cwd_once_when_osc7_is_split_across_chunks() {
        let (mut sink, _, cwd_paths) = recording_sink();

        sink.data(b"prefix \x1b]7;file://host/tm");
        assert!(cwd_paths.lock().unwrap().is_empty());
        sink.data(b"p\x07");

        assert_eq!(*cwd_paths.lock().unwrap(), vec!["/tmp".to_owned()]);
    }

    #[test]
    fn should_reject_invalid_input_when_building_domain_request() {
        let mut invalid_port = args();
        invalid_port.port = 0;
        assert!(
            matches!(to_domain(&invalid_port, None), Err(error) if error.kind == "invalid_input")
        );

        let mut empty_host = args();
        empty_host.host.clear();
        assert!(
            matches!(to_domain(&empty_host, None), Err(error) if error.kind == "invalid_input")
        );

        let mut zero_rows = args();
        zero_rows.rows = 0;
        assert!(matches!(to_domain(&zero_rows, None), Err(error) if error.kind == "invalid_input"));

        let (session, size, password, passphrase, approved) = to_domain(&args(), None).unwrap();
        assert_eq!(session.host.port.get(), 22);
        assert_eq!(size, PtySize { rows: 24, cols: 80 });
        assert_eq!(
            password.map(|secret| secret.expose().to_owned()),
            Some("pw-secret".to_owned())
        );
        assert_eq!(
            passphrase.map(|secret| secret.expose().to_owned()),
            Some("pass-secret".to_owned())
        );
        assert!(approved.is_none());
    }

    #[test]
    fn should_load_stored_session_when_connecting_with_session_id() {
        let repository = SqliteSessionRepository::in_memory().unwrap();
        let stored = Session::new(
            SessionId::new(7),
            "prod",
            Host {
                hostname: Hostname::new("guardado.example").unwrap(),
                port: Port::new(2222).unwrap(),
                username: Username::new("bob").unwrap(),
            },
            AuthMethod::Password { remember: true },
            None,
        )
        .unwrap();
        block_on(repository.save_session(&stored)).unwrap();

        let mut request = args();
        request.session_id = Some(7);
        request.host.clear();
        request.port = 0;
        request.auth = AuthArgs::Key {
            path: String::new(),
            has_passphrase: false,
        };
        let loaded = block_on(session_for_connect(&repository, &request))
            .unwrap()
            .unwrap();
        let (resolved, _, _, _, _) = to_domain(&request, Some(loaded)).unwrap();
        assert_eq!(resolved.id, SessionId::new(7));
        assert_eq!(resolved.host.hostname.as_str(), "guardado.example");

        request.session_id = Some(99);
        assert_eq!(
            block_on(session_for_connect(&repository, &request))
                .unwrap_err()
                .kind(),
            "not_found"
        );
    }

    #[test]
    fn should_map_every_connect_outcome_when_serializing_response() {
        let connected: ConnectResponse = ConnectOutcome::Connected {
            connection: ConnectionId::new(1),
            channel: ChannelId::new(2),
        }
        .into();
        assert_eq!(
            serde_json::to_string(&connected).unwrap(),
            r#"{"kind":"connected","connection_id":1,"channel_id":2}"#
        );
        let approval: ConnectResponse = ConnectOutcome::NeedsHostKeyApproval {
            fingerprint: HostKeyFingerprint::new("SHA256:abc").unwrap(),
        }
        .into();
        assert_eq!(
            serde_json::to_string(&approval).unwrap(),
            r#"{"kind":"needs_host_key_approval","fingerprint":"SHA256:abc"}"#
        );
        for (outcome, kind) in [
            (ConnectOutcome::NeedsPassword, "needs_password"),
            (ConnectOutcome::NeedsPassphrase, "needs_passphrase"),
        ] {
            let response: ConnectResponse = outcome.into();
            assert_eq!(
                serde_json::to_string(&response).unwrap(),
                format!(r#"{{"kind":"{kind}"}}"#)
            );
        }
    }

    #[test]
    fn should_map_domain_errors_when_connect_fails() {
        let kind = |error| CommandError::from_connect(error).kind;
        assert_eq!(
            kind(ConnectError::HostKeyChanged {
                fingerprint: HostKeyFingerprint::new("SHA256:abc").unwrap(),
            }),
            "host_key_changed"
        );
        assert_eq!(
            kind(ConnectError::Transport(
                TransportError::AuthenticationRejected
            )),
            "auth_rejected"
        );
        assert_eq!(
            kind(ConnectError::Transport(TransportError::NotConnected)),
            "not_connected"
        );
        assert_eq!(kind(ConnectError::Transport(TransportError::Connect)), "io");
        assert_eq!(
            kind(ConnectError::Key(KeyError::WrongPassphrase)),
            "wrong_passphrase"
        );
        assert_eq!(kind(ConnectError::Key(KeyError::NotFound)), "io");
        assert_eq!(kind(ConnectError::Secret(SecretError::Io)), "io");
        assert_eq!(kind(ConnectError::HostKey(HostKeyError::Io)), "io");
    }

    #[test]
    fn should_hide_arguments_secrets_when_serializing_responses() {
        let request = args();
        let response: ConnectResponse = ConnectOutcome::NeedsPassword.into();
        let error = CommandError::from_connect(ConnectError::Transport(TransportError::Io));
        let json = format!(
            "{}{}",
            serde_json::to_string(&response).unwrap(),
            serde_json::to_string(&error).unwrap()
        );
        assert!(!json.contains("pw-secret"));
        assert!(!json.contains("pass-secret"));
        let debug = format!("{:?}", request);
        assert!(!debug.contains("pw-secret"));
        assert!(!debug.contains("pass-secret"));
        assert!(debug.contains("[REDACTED]"));
    }
}
