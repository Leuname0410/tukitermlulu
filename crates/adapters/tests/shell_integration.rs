#![cfg(feature = "integration")]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tuki_adapters::cwd_tracker::Osc7Tracker;
use tuki_adapters::echo_filter::EchoFilter;
use tuki_adapters::ssh_transport::RusshTransport;
use tuki_application::shell_integration::{PROBE_COMMAND, ShellKind, classify, integration_line};
use tuki_core::host::{Host, Hostname, Port, Username};
use tuki_core::ports::{
    ChannelId, Connected, Credential, CwdTracker, PtySink, PtySize, SecretString, SshTransport,
};
use tuki_core::remote_path::RemotePath;

/// Salida visible y último cwd detectado por el tracker, que ve todos los bytes.
#[derive(Clone)]
struct SessionState {
    visible: Arc<Mutex<Vec<u8>>>,
    cwd: Arc<Mutex<Option<RemotePath>>>,
}

impl SessionState {
    fn contains(&self, text: &str) -> bool {
        let bytes = self.visible.lock().unwrap();
        String::from_utf8_lossy(&bytes).contains(text)
    }

    fn visible_text(&self) -> String {
        let bytes = self.visible.lock().unwrap();
        String::from_utf8_lossy(&bytes).into_owned()
    }
}

/// Sink de prueba equivalente al `ChannelSink`: tracker primero, filtro después.
#[derive(Clone)]
struct FilteredSink {
    state: SessionState,
    tracker: Arc<Mutex<Osc7Tracker>>,
    filter: Arc<Mutex<EchoFilter>>,
}

impl FilteredSink {
    fn new() -> (Self, SessionState) {
        let state = SessionState {
            visible: Arc::new(Mutex::new(Vec::new())),
            cwd: Arc::new(Mutex::new(None)),
        };
        let sink = Self {
            state: state.clone(),
            tracker: Arc::new(Mutex::new(Osc7Tracker::new())),
            filter: Arc::new(Mutex::new(EchoFilter::new())),
        };
        (sink, state)
    }

    fn activate(&self) {
        self.filter.lock().unwrap().activate();
    }
}

impl PtySink for FilteredSink {
    fn data(&mut self, bytes: &[u8]) {
        if let Some(path) = self.tracker.lock().unwrap().feed(bytes) {
            *self.state.cwd.lock().unwrap() = Some(path);
        }
        let shown = self.filter.lock().unwrap().push(bytes);
        if !shown.is_empty() {
            self.state.visible.lock().unwrap().extend_from_slice(&shown);
        }
    }

    fn closed(&mut self, _exit_status: Option<u32>) {}
}

fn host(user: &str) -> Host {
    Host {
        hostname: Hostname::new("127.0.0.1").unwrap(),
        port: Port::new(2222).unwrap(),
        username: Username::new(user).unwrap(),
    }
}

async fn connect(user: &str) -> (RusshTransport, Connected) {
    let transport = RusshTransport::new();
    let connection = transport.connect(&host(user)).await.unwrap();
    transport
        .authenticate(
            connection.id,
            Credential::Password(SecretString::new("tuki-test")),
        )
        .await
        .unwrap();
    (transport, connection)
}

async fn wait_for(state: &SessionState, label: &str, predicate: impl Fn(&SessionState) -> bool) {
    tokio::time::timeout(Duration::from_secs(12), async {
        while !predicate(state) {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timeout esperando {label}: {}", state.visible_text()));
}

/// Conecta, sonda y, si la shell es conocida, inyecta tras el silencio del prompt.
async fn integrated(
    user: &str,
    expected: ShellKind,
) -> (
    RusshTransport,
    Connected,
    ChannelId,
    FilteredSink,
    SessionState,
) {
    let (transport, connection) = connect(user).await;
    let probe = transport.exec(connection.id, PROBE_COMMAND).await.unwrap();
    assert_eq!(classify(&probe), expected, "sonda de {user}");
    let (sink, state) = FilteredSink::new();
    let channel = transport
        .open_pty(
            connection.id,
            PtySize::new(24, 80).unwrap(),
            Box::new(sink.clone()),
        )
        .await
        .unwrap();
    if expected != ShellKind::Unknown {
        tokio::time::sleep(Duration::from_millis(400)).await;
        sink.activate();
        let line = integration_line(expected).unwrap();
        transport.write(channel, line.as_bytes()).await.unwrap();
        wait_for(&state, "primer cwd", |s| s.cwd.lock().unwrap().is_some()).await;
    }
    (transport, connection, channel, sink, state)
}

#[tokio::test]
async fn should_track_cd_and_hide_injection_when_shell_is_bash_zsh_or_fish() {
    let tmp = RemotePath::new("/tmp").unwrap();
    for (user, shell) in [
        ("tuki-bash", ShellKind::Bash),
        ("tuki-zsh", ShellKind::Zsh),
        ("tuki-fish", ShellKind::Fish),
    ] {
        let (transport, connection, channel, _sink, state) = integrated(user, shell).await;
        transport.write(channel, b"cd /tmp\n").await.unwrap();
        wait_for(&state, "cwd /tmp", |s| {
            *s.cwd.lock().unwrap() == Some(tmp.clone())
        })
        .await;

        if shell == ShellKind::Bash {
            transport.write(channel, b"history\n").await.unwrap();
            tokio::time::sleep(Duration::from_millis(300)).await;
            let visible = state.visible_text();
            assert!(visible.contains("cd /tmp"), "historial: {visible:?}");
            assert!(!visible.contains("__tuki_osc7"), "historial: {visible:?}");
        }

        let visible = state.visible_text();
        assert!(!visible.contains("__tuki_osc7"), "{user}: {visible:?}");
        assert!(!visible.contains("tuki-ready"), "{user}: {visible:?}");
        assert!(transport.close(connection.id).await.is_ok());
    }
}

#[tokio::test]
async fn should_keep_ash_terminal_working_when_shell_is_unknown() {
    let (transport, connection, channel, _sink, state) =
        integrated("tuki", ShellKind::Unknown).await;

    transport.write(channel, b"echo ok\n").await.unwrap();
    wait_for(&state, "echo ok", |s| s.contains("ok")).await;

    assert!(!state.contains("__tuki_osc7"));
    assert!(!state.contains("tuki-ready"));
    assert!(transport.close(connection.id).await.is_ok());
}
