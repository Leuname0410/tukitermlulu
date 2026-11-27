//! Puente IPC de la terminal: DTOs, comandos y canal binario de salida.

use std::fmt;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::State;
use tauri::ipc::{Channel, InvokeBody, InvokeResponseBody, Request};
use tuki_adapters::key_loader::RusshKeyLoader;
use tuki_adapters::known_hosts::FileHostKeyVerifier;
use tuki_adapters::secret_store::KeyringSecretStore;
use tuki_adapters::ssh_transport::RusshTransport;
use tuki_application::connect_session::{
    ConnectError, ConnectOutcome, ConnectRequest, ConnectSession,
};
use tuki_core::auth::{AuthMethod, KeyRef};
use tuki_core::host::{Host, Hostname, Port, Username};
use tuki_core::port_errors::{KeyError, TransportError};
use tuki_core::ports::{
    ChannelId, ConnectionId, HostKeyFingerprint, PtySink, PtySize, SecretString, SshTransport,
};
use tuki_core::session::{Session, SessionId};
use zeroize::Zeroizing;

/// Estado compartido por los comandos: transporte y ruta del known_hosts privado.
pub struct AppState {
    transport: RusshTransport,
    known_hosts_path: PathBuf,
}

impl AppState {
    /// Crea el estado de la composition root.
    pub(crate) fn new(transport: RusshTransport, known_hosts_path: PathBuf) -> Self {
        Self {
            transport,
            known_hosts_path,
        }
    }
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
pub struct CommandError {
    kind: &'static str,
    message: String,
    fingerprint: Option<String>,
}

/// Aviso de cierre del PTY enviado por el canal de cierre.
#[derive(Deserialize, Serialize, Clone)]
pub struct TerminalClosed {
    exit_status: Option<u32>,
}

/// Entrega la salida del PTY como bytes raw y notifica su cierre.
pub struct ChannelSink {
    output: Channel<InvokeResponseBody>,
    closed: Channel<TerminalClosed>,
}

impl PtySink for ChannelSink {
    fn data(&mut self, bytes: &[u8]) {
        let _ = self.output.send(InvokeResponseBody::Raw(bytes.to_vec()));
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
    fn new(kind: &'static str, message: impl fmt::Display) -> Self {
        Self {
            kind,
            message: message.to_string(),
            fingerprint: None,
        }
    }

    fn invalid_input(error: impl fmt::Display) -> Self {
        Self::new("invalid_input", error)
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

/// Valida los DTOs sin tocar la red y copia los secretos a tipos envueltos.
fn to_domain(args: &ConnectArgs) -> Result<DomainParts, CommandError> {
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
    let session = Session::new(SessionId::new(0), &args.host, host, auth, None)
        .map_err(CommandError::invalid_input)?;
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

/// Conecta y abre un PTY; la salida viaja por `on_output`, el cierre por `on_closed`.
#[tauri::command]
pub async fn connect(
    state: State<'_, AppState>,
    args: ConnectArgs,
    on_output: Channel<InvokeResponseBody>,
    on_closed: Channel<TerminalClosed>,
) -> Result<ConnectResponse, CommandError> {
    let (session, pty_size, password, passphrase, approved_fingerprint) = to_domain(&args)?;
    drop(args);
    let use_case = ConnectSession::new(
        state.transport.clone(),
        FileHostKeyVerifier::new(state.known_hosts_path.clone()),
        RusshKeyLoader,
        KeyringSecretStore::new("tukitermlulu"),
    );
    let request = ConnectRequest {
        session: &session,
        password: password.as_ref(),
        passphrase: passphrase.as_ref(),
        approved_fingerprint: approved_fingerprint.as_ref(),
        pty_size,
        sink: Box::new(ChannelSink {
            output: on_output,
            closed: on_closed,
        }),
    };
    let outcome = use_case
        .execute(request)
        .await
        .map_err(CommandError::from_connect)?;
    Ok(outcome.into())
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

/// Cierra una conexión y sus PTY asociados.
#[tauri::command]
pub async fn disconnect(
    state: State<'_, AppState>,
    connection_id: u64,
) -> Result<(), CommandError> {
    state
        .transport
        .close(ConnectionId::new(connection_id))
        .await
        .map_err(CommandError::from_transport)
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use tauri::ipc::{Channel, InvokeResponseBody};
    use tuki_core::port_errors::{HostKeyError, KeyError, SecretError, TransportError};
    use tuki_core::ports::{ChannelId, ConnectionId, HostKeyFingerprint, PtySize};

    use super::*;

    fn args() -> ConnectArgs {
        ConnectArgs {
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
        let mut sink = ChannelSink { output, closed };

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
    fn should_reject_invalid_input_when_building_domain_request() {
        let mut invalid_port = args();
        invalid_port.port = 0;
        assert!(matches!(to_domain(&invalid_port), Err(error) if error.kind == "invalid_input"));

        let mut empty_host = args();
        empty_host.host.clear();
        assert!(matches!(to_domain(&empty_host), Err(error) if error.kind == "invalid_input"));

        let mut zero_rows = args();
        zero_rows.rows = 0;
        assert!(matches!(to_domain(&zero_rows), Err(error) if error.kind == "invalid_input"));

        let (session, size, password, passphrase, approved) = to_domain(&args()).unwrap();
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
