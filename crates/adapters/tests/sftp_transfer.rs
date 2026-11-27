#![cfg(feature = "integration")]

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use tuki_adapters::sftp::RusshFileSystem;
use tuki_adapters::ssh_transport::RusshTransport;
use tuki_core::host::{Host, Hostname, Port, Username};
use tuki_core::port_errors::{FsError, TransferError};
use tuki_core::ports::{
    CancelToken, Connected, ConnectionId, Credential, FileTransfer, RemoteFileSystem, SecretString,
    SshTransport, TransferProgress,
};
use tuki_core::remote_path::RemotePath;

const CHUNK: usize = 256 * 1024;
const SIZE: u64 = 32 * 1024 * 1024;
static NEXT: AtomicU64 = AtomicU64::new(1);

fn host() -> Host {
    Host {
        hostname: Hostname::new("127.0.0.1").unwrap(),
        port: Port::new(2222).unwrap(),
        username: Username::new("tuki").unwrap(),
    }
}

fn remote(value: String) -> RemotePath {
    RemotePath::new(&value).unwrap()
}

fn unique_remote() -> RemotePath {
    remote(format!(
        "/home/tuki/t22a-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

fn local_part(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(".part");
    PathBuf::from(name)
}

fn remote_part(path: &RemotePath) -> RemotePath {
    remote(format!("{}.part", path.as_str()))
}

async fn fs() -> (RusshTransport, Connected, RusshFileSystem) {
    let transport = RusshTransport::new();
    let connected = transport.connect(&host()).await.unwrap();
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

/// Registra cada llamada y cancela en la primera, para probar la cancelación entre bloques.
struct Progress {
    calls: Arc<Mutex<Vec<(u64, Option<u64>)>>>,
    cancel: Option<CancelToken>,
}

impl Progress {
    fn new() -> (Self, Arc<Mutex<Vec<(u64, Option<u64>)>>>) {
        let calls = Arc::new(Mutex::new(Vec::new()));
        (
            Self {
                calls: Arc::clone(&calls),
                cancel: None,
            },
            calls,
        )
    }

    fn cancelling(token: CancelToken) -> Self {
        Self {
            calls: Arc::new(Mutex::new(Vec::new())),
            cancel: Some(token),
        }
    }
}

impl TransferProgress for Progress {
    fn progress(&self, transferred: u64, total: Option<u64>) {
        let mut calls = self.calls.lock().unwrap();
        calls.push((transferred, total));
        if calls.len() == 1 {
            if let Some(token) = &self.cancel {
                token.cancel();
            }
        }
    }
}

/// Escribe contenido pseudoaleatorio por bloques con una semilla fija.
fn write_random(path: &Path, size: u64) {
    let mut file = std::fs::File::create(path).unwrap();
    let mut buffer = vec![0_u8; CHUNK];
    let mut state = 0x9E37_79B9_7F4A_7C15_u64;
    let mut left = size;
    while left > 0 {
        let len = usize::try_from(left.min(CHUNK as u64)).unwrap();
        for byte in &mut buffer[..len] {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            *byte = (state >> 33) as u8;
        }
        std::io::Write::write_all(&mut file, &buffer[..len]).unwrap();
        left -= len as u64;
    }
}

/// Hash FNV-1a calculado por bloques, sin cargar el archivo completo.
fn hash(path: &Path) -> u64 {
    let mut file = std::fs::File::open(path).unwrap();
    let mut buffer = vec![0_u8; CHUNK];
    let mut value = 0xcbf2_9ce4_8422_2325_u64;
    loop {
        let read = file.read(&mut buffer).unwrap();
        if read == 0 {
            return value;
        }
        for byte in &buffer[..read] {
            value = (value ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3);
        }
    }
}

async fn missing(fs: &RusshFileSystem, conn: ConnectionId, path: &RemotePath) -> bool {
    matches!(fs.stat(conn, path).await, Err(FsError::NotFound))
}

#[tokio::test]
async fn should_round_trip_and_report_increasing_progress_when_transferring_32mib() {
    let (transport, connected, fs) = fs().await;
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.bin");
    let downloaded = dir.path().join("downloaded.bin");
    let target = unique_remote();
    write_random(&source, SIZE);

    let (upload, upload_calls) = Progress::new();
    assert_eq!(
        fs.upload(
            connected.id,
            &source,
            &target,
            &upload,
            &CancelToken::default()
        )
        .await
        .unwrap(),
        SIZE
    );
    let (download, download_calls) = Progress::new();
    assert_eq!(
        fs.download(
            connected.id,
            &target,
            &downloaded,
            &download,
            &CancelToken::default()
        )
        .await
        .unwrap(),
        SIZE
    );

    assert_eq!(hash(&source), hash(&downloaded));
    for calls in [&upload_calls, &download_calls] {
        let values = calls.lock().unwrap();
        assert!(values.windows(2).all(|w| w[1].0 >= w[0].0));
        assert_eq!(values.last(), Some(&(SIZE, Some(SIZE))));
    }
    assert!(fs.remove(connected.id, &target).await.is_ok());
    assert!(transport.close(connected.id).await.is_ok());
}

#[tokio::test]
async fn should_remove_partial_file_when_download_is_cancelled() {
    let (transport, connected, fs) = fs().await;
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.bin");
    let target = unique_remote();
    write_random(&source, (2 * CHUNK) as u64);
    fs.upload(
        connected.id,
        &source,
        &target,
        &Progress::new().0,
        &CancelToken::default(),
    )
    .await
    .unwrap();
    let downloaded = dir.path().join("downloaded.bin");
    let token = CancelToken::default();

    let result = fs
        .download(
            connected.id,
            &target,
            &downloaded,
            &Progress::cancelling(token.clone()),
            &token,
        )
        .await;

    assert!(matches!(result, Err(TransferError::Cancelled)));
    assert!(!downloaded.exists());
    assert!(!local_part(&downloaded).exists());
    assert!(fs.remove(connected.id, &target).await.is_ok());
    assert!(transport.close(connected.id).await.is_ok());
}

#[tokio::test]
async fn should_remove_partial_file_when_upload_is_cancelled() {
    let (transport, connected, fs) = fs().await;
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.bin");
    write_random(&source, (2 * CHUNK) as u64);
    let target = unique_remote();
    let token = CancelToken::default();

    let result = fs
        .upload(
            connected.id,
            &source,
            &target,
            &Progress::cancelling(token.clone()),
            &token,
        )
        .await;

    assert!(matches!(result, Err(TransferError::Cancelled)));
    assert!(missing(&fs, connected.id, &target).await);
    assert!(missing(&fs, connected.id, &remote_part(&target)).await);
    assert!(transport.close(connected.id).await.is_ok());
}

#[tokio::test]
async fn should_keep_local_destination_when_it_already_exists() {
    let (transport, connected, fs) = fs().await;
    let dir = tempfile::tempdir().unwrap();
    let existing = dir.path().join("existing.bin");
    std::fs::write(&existing, b"original").unwrap();

    let result = fs
        .download(
            connected.id,
            &remote("/etc/hostname".to_owned()),
            &existing,
            &Progress::new().0,
            &CancelToken::default(),
        )
        .await;

    assert!(matches!(result, Err(TransferError::AlreadyExists)));
    assert_eq!(std::fs::read(&existing).unwrap(), b"original");
    assert!(!local_part(&existing).exists());
    assert!(transport.close(connected.id).await.is_ok());
}

#[tokio::test]
async fn should_keep_remote_destination_when_it_already_exists() {
    let (transport, connected, fs) = fs().await;
    let dir = tempfile::tempdir().unwrap();
    let first = dir.path().join("first.bin");
    let second = dir.path().join("second.bin");
    std::fs::write(&first, b"first").unwrap();
    std::fs::write(&second, b"second").unwrap();
    let target = unique_remote();
    fs.upload(
        connected.id,
        &first,
        &target,
        &Progress::new().0,
        &CancelToken::default(),
    )
    .await
    .unwrap();

    let result = fs
        .upload(
            connected.id,
            &second,
            &target,
            &Progress::new().0,
            &CancelToken::default(),
        )
        .await;

    assert!(matches!(result, Err(TransferError::AlreadyExists)));
    assert_eq!(fs.stat(connected.id, &target).await.unwrap().size, 5);
    assert!(missing(&fs, connected.id, &remote_part(&target)).await);
    assert!(fs.remove(connected.id, &target).await.is_ok());
    assert!(transport.close(connected.id).await.is_ok());
}

#[tokio::test]
async fn should_report_not_found_when_downloading_a_missing_remote_path() {
    let (transport, connected, fs) = fs().await;
    let dir = tempfile::tempdir().unwrap();
    let downloaded = dir.path().join("downloaded.bin");
    let target = unique_remote();

    let result = fs
        .download(
            connected.id,
            &target,
            &downloaded,
            &Progress::new().0,
            &CancelToken::default(),
        )
        .await;

    assert!(matches!(result, Err(TransferError::NotFound)));
    assert!(!downloaded.exists());
    assert!(!local_part(&downloaded).exists());
    assert!(transport.close(connected.id).await.is_ok());
}

#[tokio::test]
async fn should_report_permission_denied_when_uploading_to_root() {
    let (transport, connected, fs) = fs().await;
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.bin");
    std::fs::write(&source, b"data").unwrap();

    let result = fs
        .upload(
            connected.id,
            &source,
            &remote("/root/x".to_owned()),
            &Progress::new().0,
            &CancelToken::default(),
        )
        .await;

    assert!(matches!(result, Err(TransferError::PermissionDenied)));
    assert!(transport.close(connected.id).await.is_ok());
}

#[tokio::test]
async fn should_keep_preexisting_local_part_when_downloading() {
    let (transport, connected, fs) = fs().await;
    let dir = tempfile::tempdir().unwrap();
    let downloaded = dir.path().join("downloaded.bin");
    let part = local_part(&downloaded);
    std::fs::write(&part, b"keep me").unwrap();

    let result = fs
        .download(
            connected.id,
            &remote("/etc/hostname".to_owned()),
            &downloaded,
            &Progress::new().0,
            &CancelToken::default(),
        )
        .await;

    assert!(matches!(result, Err(TransferError::AlreadyExists)));
    assert_eq!(std::fs::read(&part).unwrap(), b"keep me");
    assert!(!downloaded.exists());
    assert!(transport.close(connected.id).await.is_ok());
}

#[tokio::test]
async fn should_keep_preexisting_remote_part_when_uploading() {
    let (transport, connected, fs) = fs().await;
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.bin");
    let original = dir.path().join("original.part");
    std::fs::write(&source, b"new data").unwrap();
    std::fs::write(&original, b"keep me").unwrap();
    let target = unique_remote();
    let part = remote_part(&target);
    fs.upload(
        connected.id,
        &original,
        &part,
        &Progress::new().0,
        &CancelToken::default(),
    )
    .await
    .unwrap();

    let result = fs
        .upload(
            connected.id,
            &source,
            &target,
            &Progress::new().0,
            &CancelToken::default(),
        )
        .await;

    assert!(matches!(result, Err(TransferError::AlreadyExists)));
    let restored = dir.path().join("restored.bin");
    fs.download(
        connected.id,
        &part,
        &restored,
        &Progress::new().0,
        &CancelToken::default(),
    )
    .await
    .unwrap();
    assert_eq!(std::fs::read(&restored).unwrap(), b"keep me");
    assert!(missing(&fs, connected.id, &target).await);
    assert!(fs.remove(connected.id, &part).await.is_ok());
    assert!(transport.close(connected.id).await.is_ok());
}

#[tokio::test]
async fn should_transfer_odd_sized_file_with_identical_content() {
    let (transport, connected, fs) = fs().await;
    let dir = tempfile::tempdir().unwrap();
    let size = 8 * 1024 * 1024 + 123;
    let source = dir.path().join("odd-source.bin");
    let downloaded = dir.path().join("odd-downloaded.bin");
    let target = unique_remote();
    write_random(&source, size);

    let transferred_up = fs
        .upload(
            connected.id,
            &source,
            &target,
            &Progress::new().0,
            &CancelToken::default(),
        )
        .await
        .unwrap();
    let transferred_down = fs
        .download(
            connected.id,
            &target,
            &downloaded,
            &Progress::new().0,
            &CancelToken::default(),
        )
        .await
        .unwrap();

    assert_eq!(transferred_up, size);
    assert_eq!(transferred_down, size);
    assert_eq!(hash(&source), hash(&downloaded));
    assert!(fs.remove(connected.id, &target).await.is_ok());
    assert!(transport.close(connected.id).await.is_ok());
}

#[tokio::test]
async fn should_report_monotonic_progress_and_final_size_when_transferring_odd_sized_file() {
    let (transport, connected, fs) = fs().await;
    let dir = tempfile::tempdir().unwrap();
    let size = 8 * 1024 * 1024 + 123;
    let source = dir.path().join("odd-progress.bin");
    let downloaded = dir.path().join("odd-progress-down.bin");
    let target = unique_remote();
    write_random(&source, size);

    let (upload, upload_calls) = Progress::new();
    fs.upload(
        connected.id,
        &source,
        &target,
        &upload,
        &CancelToken::default(),
    )
    .await
    .unwrap();
    let (download, download_calls) = Progress::new();
    fs.download(
        connected.id,
        &target,
        &downloaded,
        &download,
        &CancelToken::default(),
    )
    .await
    .unwrap();

    for calls in [&upload_calls, &download_calls] {
        let values = calls.lock().unwrap();
        assert!(!values.is_empty());
        assert!(values.windows(2).all(|w| w[1].0 >= w[0].0));
        assert_eq!(values.last(), Some(&(size, Some(size))));
    }
    assert!(fs.remove(connected.id, &target).await.is_ok());
    assert!(transport.close(connected.id).await.is_ok());
}
