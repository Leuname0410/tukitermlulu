#![cfg(feature = "integration")]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::Notify;
use tuki_adapters::sftp::RusshFileSystem;
use tuki_adapters::ssh_transport::RusshTransport;
use tuki_core::host::{Host, Hostname, Port, Username};
use tuki_core::port_errors::FsError;
use tuki_core::ports::{
    Connected, ConnectionId, Credential, PtySink, PtySize, RemoteFileSystem, SecretString,
    SshTransport,
};
use tuki_core::remote_path::RemotePath;

static NEXT_SUFFIX: AtomicU64 = AtomicU64::new(1);

fn test_host() -> Host {
    Host {
        hostname: Hostname::new("127.0.0.1").unwrap(),
        port: Port::new(2222).unwrap(),
        username: Username::new("tuki").unwrap(),
    }
}

fn path(value: &str) -> RemotePath {
    RemotePath::new(value).unwrap()
}

/// Ruta única bajo `/home/tuki` para no colisionar con ejecuciones previas.
fn unique_path() -> RemotePath {
    let suffix = NEXT_SUFFIX.fetch_add(1, Ordering::Relaxed);
    path(&format!("/home/tuki/t21-{}-{suffix}", std::process::id()))
}

async fn authenticated_fs() -> (RusshTransport, Connected, RusshFileSystem) {
    let transport = RusshTransport::new();
    let connected = transport.connect(&test_host()).await.unwrap();
    transport
        .authenticate(
            connected.id,
            Credential::Password(SecretString::new("tuki-test")),
        )
        .await
        .unwrap();
    let fs = RusshFileSystem::new(transport.clone());
    (transport, connected, fs)
}

/// Estado del sink de prueba: bytes recibidos, cierre observado y notificaciones.
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

async fn wait_for_close(state: &SinkState) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let notified = state.notify.notified();
            tokio::pin!(notified);
            if state.closed.lock().unwrap().is_some() {
                return;
            }
            notified.await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("el shell no se cerró en 5 s"));
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

/// Crea un archivo remoto con el shell de la misma conexión SSH.
async fn create_file(transport: &RusshTransport, conn: ConnectionId, target: &RemotePath) {
    let (sink, state) = RecordingSink::new();
    let channel = transport
        .open_pty(conn, PtySize::new(24, 80).unwrap(), Box::new(sink))
        .await
        .unwrap();
    let command = format!("touch {}\nexit\n", target.as_str());
    transport.write(channel, command.as_bytes()).await.unwrap();
    wait_for_close(&state).await;
}

#[tokio::test]
async fn should_list_and_stat_created_directory_when_mkdir() {
    let (transport, connected, fs) = authenticated_fs().await;
    let directory = unique_path();

    fs.mkdir(connected.id, &directory).await.unwrap();

    let parent = directory.parent().unwrap();
    let entries = fs.list(connected.id, &parent).await.unwrap();
    assert!(
        entries
            .iter()
            .any(|entry| entry.path == directory && entry.is_dir)
    );
    let stat = fs.stat(connected.id, &directory).await.unwrap();
    assert!(stat.is_dir);
    assert!(transport.close(connected.id).await.is_ok());
    let _ = fs.remove(connected.id, &directory).await;
}

#[tokio::test]
async fn should_report_file_size_when_stating_existing_file() {
    let (transport, connected, fs) = authenticated_fs().await;

    let stat = fs.stat(connected.id, &path("/etc/hostname")).await.unwrap();

    assert!(!stat.is_dir);
    assert!(stat.size > 0);
    assert!(transport.close(connected.id).await.is_ok());
}

#[tokio::test]
async fn should_report_not_found_when_stating_missing_path() {
    let (transport, connected, fs) = authenticated_fs().await;

    let result = fs.stat(connected.id, &unique_path()).await;

    assert!(matches!(result, Err(FsError::NotFound)));
    assert!(transport.close(connected.id).await.is_ok());
}

#[tokio::test]
async fn should_report_not_found_when_renaming_directory() {
    let (transport, connected, fs) = authenticated_fs().await;
    let source = unique_path();
    let target = unique_path();
    fs.mkdir(connected.id, &source).await.unwrap();

    fs.rename(connected.id, &source, &target).await.unwrap();

    assert!(matches!(
        fs.stat(connected.id, &source).await,
        Err(FsError::NotFound)
    ));
    assert!(fs.stat(connected.id, &target).await.unwrap().is_dir);
    assert!(transport.close(connected.id).await.is_ok());
    let _ = fs.remove(connected.id, &target).await;
}

#[tokio::test]
async fn should_remove_empty_directory_when_directory_has_no_entries() {
    let (transport, connected, fs) = authenticated_fs().await;
    let directory = unique_path();
    fs.mkdir(connected.id, &directory).await.unwrap();

    fs.remove(connected.id, &directory).await.unwrap();

    assert!(matches!(
        fs.stat(connected.id, &directory).await,
        Err(FsError::NotFound)
    ));
    assert!(transport.close(connected.id).await.is_ok());
}

#[tokio::test]
async fn should_remove_file_when_path_is_a_regular_file() {
    let (transport, connected, fs) = authenticated_fs().await;
    let file = unique_path();
    create_file(&transport, connected.id, &file).await;
    assert!(!fs.stat(connected.id, &file).await.unwrap().is_dir);

    fs.remove(connected.id, &file).await.unwrap();

    assert!(matches!(
        fs.stat(connected.id, &file).await,
        Err(FsError::NotFound)
    ));
    assert!(transport.close(connected.id).await.is_ok());
}

#[tokio::test]
async fn should_report_operation_when_removing_non_empty_directory() {
    let (transport, connected, fs) = authenticated_fs().await;
    let directory = unique_path();
    fs.mkdir(connected.id, &directory).await.unwrap();
    create_file(&transport, connected.id, &directory.join("child").unwrap()).await;

    let result = fs.remove(connected.id, &directory).await;

    assert!(matches!(result, Err(FsError::Operation)));
    assert!(transport.close(connected.id).await.is_ok());
    let _ = fs
        .remove(connected.id, &directory.join("child").unwrap())
        .await;
    let _ = fs.remove(connected.id, &directory).await;
}

#[tokio::test]
async fn should_report_permission_denied_when_listing_root() {
    let (transport, connected, fs) = authenticated_fs().await;

    let result = fs.list(connected.id, &path("/root")).await;

    assert!(matches!(result, Err(FsError::PermissionDenied)));
    assert!(transport.close(connected.id).await.is_ok());
}

#[tokio::test]
async fn should_keep_terminal_working_when_sftp_is_used_over_same_connection() {
    let (transport, connected, fs) = authenticated_fs().await;
    let directory = unique_path();
    let (sink, state) = RecordingSink::new();
    let channel = transport
        .open_pty(connected.id, PtySize::new(24, 80).unwrap(), Box::new(sink))
        .await
        .unwrap();

    fs.mkdir(connected.id, &directory).await.unwrap();
    transport
        .write(channel, b"echo pty-and-sftp\n")
        .await
        .unwrap();

    let output = wait_for_output(&state, b"pty-and-sftp").await;
    assert!(String::from_utf8_lossy(&output).contains("pty-and-sftp"));
    assert!(transport.close(connected.id).await.is_ok());
    let _ = fs.remove(connected.id, &directory).await;
}

#[tokio::test]
async fn should_report_operation_when_using_sftp_after_connection_is_closed() {
    let (transport, connected, fs) = authenticated_fs().await;
    assert!(transport.close(connected.id).await.is_ok());

    let result = fs.list(connected.id, &path("/home/tuki")).await;

    assert!(matches!(result, Err(FsError::Operation)));
}
