//! Puente IPC de las transferencias: DTOs, cola, worker y eventos limitados.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::Notify;
use tuki_application::transfer_queue::{
    TransferId, TransferJob, TransferKind, TransferQueue, TransferState, local_download_target,
};
use tuki_core::port_errors::TransferError;
use tuki_core::ports::{ConnectionId, FileTransfer, TransferProgress};
use tuki_core::remote_path::RemotePath;

use crate::terminal::{AppState, CommandError};

/// Estado de una transferencia enviado a la UI; nunca incluye bytes de archivo.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TransferDto {
    id: u64,
    connection_id: u64,
    direction: &'static str,
    remote: String,
    local: String,
    state: &'static str,
    transferred: u64,
    total: Option<u64>,
    error: Option<String>,
}

/// Destino de los eventos de la cola. `AppHandle` es la implementación real.
pub(crate) trait TransferEvents: Send + Sync {
    fn emit(&self, jobs: &[TransferDto]);
}

impl TransferEvents for AppHandle {
    fn emit(&self, jobs: &[TransferDto]) {
        let _ = Emitter::emit(self, "transfers", jobs.to_vec());
    }
}

/// Adquiere la cola sin propagar el envenenamiento del `Mutex`.
fn lock_queue(transfers: &Arc<Mutex<TransferQueue>>) -> MutexGuard<'_, TransferQueue> {
    transfers
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Proyecta cada trabajo al DTO serializable.
fn to_dto(job: &TransferJob) -> TransferDto {
    let (direction, remote, local) = match &job.kind {
        TransferKind::Download { remote, local } => (
            "download",
            remote.as_str(),
            local.to_string_lossy().into_owned(),
        ),
        TransferKind::Upload { local, remote } => (
            "upload",
            remote.as_str(),
            local.to_string_lossy().into_owned(),
        ),
    };
    let (state, transferred, total, error) = match &job.state {
        TransferState::Queued => ("queued", 0, None, None),
        TransferState::Running { transferred, total } => ("running", *transferred, *total, None),
        TransferState::Done { bytes } => ("done", *bytes, Some(*bytes), None),
        TransferState::Failed(error) => ("failed", 0, None, Some(error.to_string())),
        TransferState::Cancelled => ("cancelled", 0, None, None),
    };
    TransferDto {
        id: job.id.get(),
        connection_id: job.conn.get(),
        direction,
        remote: remote.to_owned(),
        local,
        state,
        transferred,
        total,
        error,
    }
}

fn snapshot(queue: &TransferQueue) -> Vec<TransferDto> {
    queue.jobs().iter().map(to_dto).collect()
}

/// Publica el estado completo de la cola; los fallos de emisión se ignoran.
pub(crate) fn emit_transfers(events: &dyn TransferEvents, queue: &TransferQueue) {
    events.emit(&snapshot(queue));
}

/// Limita el progreso a 10 eventos por segundo y trabajo.
pub(crate) struct Throttle;

impl Throttle {
    const INTERVAL: Duration = Duration::from_millis(100);

    /// Decide si el instante actual merece un nuevo evento de progreso.
    pub(crate) fn should_emit(last: Instant, now: Instant) -> bool {
        now.checked_duration_since(last)
            .is_some_and(|elapsed| elapsed >= Self::INTERVAL)
    }
}

/// Progreso sin I/O que actualiza la cola y emite eventos acotados.
struct QueueProgress {
    transfers: Arc<Mutex<TransferQueue>>,
    events: Arc<dyn TransferEvents>,
    id: TransferId,
    last: Mutex<Option<Instant>>,
}

impl QueueProgress {
    fn new(
        transfers: Arc<Mutex<TransferQueue>>,
        events: Arc<dyn TransferEvents>,
        id: TransferId,
    ) -> Self {
        Self {
            transfers,
            events,
            id,
            last: Mutex::new(None),
        }
    }
}

impl TransferProgress for QueueProgress {
    fn progress(&self, transferred: u64, total: Option<u64>) {
        let now = Instant::now();
        let should_emit = {
            let mut last = self.last.lock().unwrap_or_else(|e| e.into_inner());
            match *last {
                Some(previous) if !Throttle::should_emit(previous, now) => false,
                _ => {
                    *last = Some(now);
                    true
                }
            }
        };
        let jobs = {
            let mut queue = lock_queue(&self.transfers);
            queue.progress(self.id, transferred, total);
            if should_emit {
                snapshot(&queue)
            } else {
                Vec::new()
            }
        };
        if should_emit {
            self.events.emit(&jobs);
        }
    }
}

/// Ejecuta el trabajo indicado usando el adaptador inyectado.
async fn run_transfer<F: FileTransfer>(
    fs: &F,
    job: &TransferJob,
    progress: &dyn TransferProgress,
) -> Result<u64, TransferError> {
    match &job.kind {
        TransferKind::Download { remote, local } => {
            fs.download(job.conn, remote, local, progress, &job.cancel)
                .await
        }
        TransferKind::Upload { local, remote } => {
            fs.upload(job.conn, local, remote, progress, &job.cancel)
                .await
        }
    }
}

/// Worker único: saca un trabajo, espera su transferencia y publica cada cambio.
pub(crate) async fn transfer_worker<F: FileTransfer + 'static>(
    transfers: Arc<Mutex<TransferQueue>>,
    wake: Arc<Notify>,
    fs: F,
    events: Arc<dyn TransferEvents>,
) {
    loop {
        let job = lock_queue(&transfers).start_next();
        let Some(job) = job else {
            wake.notified().await;
            continue;
        };
        let jobs = {
            let queue = lock_queue(&transfers);
            snapshot(&queue)
        };
        events.emit(&jobs);

        let progress = QueueProgress::new(Arc::clone(&transfers), Arc::clone(&events), job.id);
        let result = run_transfer(&fs, &job, &progress).await;
        let jobs = {
            let mut queue = lock_queue(&transfers);
            queue.finish(job.id, result);
            snapshot(&queue)
        };
        events.emit(&jobs);
    }
}

/// Comprueba que el directorio local de descarga exista y sea directorio.
async fn ensure_local_dir(path: &str) -> Result<(), CommandError> {
    match tokio::fs::metadata(path).await {
        Ok(metadata) if metadata.is_dir() => Ok(()),
        Ok(_) => Err(CommandError::invalid_input(
            "local directory must be a directory",
        )),
        Err(_) => Err(CommandError::invalid_input(
            "local directory is not accessible",
        )),
    }
}

/// Comprueba que la ruta local de subida exista y sea un archivo.
async fn ensure_local_file(path: &str) -> Result<(), CommandError> {
    match tokio::fs::metadata(path).await {
        Ok(metadata) if metadata.is_file() => Ok(()),
        Ok(_) => Err(CommandError::invalid_input("local path must be a file")),
        Err(_) => Err(CommandError::invalid_input("local file is not accessible")),
    }
}

/// Resuelve la ruta remota y el destino local seguro de una descarga.
fn download_target(remote: &str, local_dir: &str) -> Result<(RemotePath, PathBuf), CommandError> {
    let remote = RemotePath::new(remote).map_err(CommandError::invalid_input)?;
    let local = local_download_target(Path::new(local_dir), &remote)
        .map_err(CommandError::invalid_input)?;
    Ok((remote, local))
}

/// Compone la ruta remota de subida rechazando nombres inseguros.
fn remote_upload_path(remote_dir: &str, name: &str) -> Result<RemotePath, CommandError> {
    let dir = RemotePath::new(remote_dir).map_err(CommandError::invalid_input)?;
    dir.join(name).map_err(CommandError::invalid_input)
}

/// Resuelve la ruta local y la remota de una subida.
fn upload_target(
    local_path: &str,
    remote_dir: &str,
) -> Result<(PathBuf, RemotePath), CommandError> {
    let name = Path::new(local_path)
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| CommandError::invalid_input("local file name is invalid"))?;
    let remote = remote_upload_path(remote_dir, name)?;
    Ok((PathBuf::from(local_path), remote))
}

/// Encola una descarga en un directorio local existente.
#[tauri::command]
pub async fn enqueue_download(
    state: State<'_, AppState>,
    app: AppHandle,
    connection_id: u64,
    remote: String,
    local_dir: String,
) -> Result<u64, CommandError> {
    ensure_local_dir(&local_dir).await?;
    let (remote, local) = download_target(&remote, &local_dir)?;
    let id = {
        let mut queue = lock_queue(&state.transfers);
        let id = queue
            .enqueue(
                ConnectionId::new(connection_id),
                TransferKind::Download { remote, local },
            )
            .get();
        emit_transfers(&app, &queue);
        id
    };
    state.notify.notify_one();
    Ok(id)
}

/// Encola una subida de un archivo local a un directorio remoto.
#[tauri::command]
pub async fn enqueue_upload(
    state: State<'_, AppState>,
    app: AppHandle,
    connection_id: u64,
    local_path: String,
    remote_dir: String,
) -> Result<u64, CommandError> {
    ensure_local_file(&local_path).await?;
    let (local, remote) = upload_target(&local_path, &remote_dir)?;
    let id = {
        let mut queue = lock_queue(&state.transfers);
        let id = queue
            .enqueue(
                ConnectionId::new(connection_id),
                TransferKind::Upload { local, remote },
            )
            .get();
        emit_transfers(&app, &queue);
        id
    };
    state.notify.notify_one();
    Ok(id)
}

/// Cancela un trabajo encolado o señala la cancelación al worker.
#[tauri::command]
pub fn cancel_transfer(
    state: State<'_, AppState>,
    app: AppHandle,
    id: u64,
) -> Result<(), CommandError> {
    let mut queue = lock_queue(&state.transfers);
    if !queue.cancel(TransferId::new(id)) {
        return Err(CommandError::not_found("transfer not found"));
    }
    emit_transfers(&app, &queue);
    Ok(())
}

/// Elimina los trabajos terminados, fallidos o cancelados.
#[tauri::command]
pub fn clear_transfers(state: State<'_, AppState>, app: AppHandle) {
    let mut queue = lock_queue(&state.transfers);
    queue.clear_finished();
    emit_transfers(&app, &queue);
}

/// Devuelve el estado completo de la cola.
#[tauri::command]
pub fn list_transfers(state: State<'_, AppState>) -> Vec<TransferDto> {
    snapshot(&lock_queue(&state.transfers))
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::path::Path;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use tuki_application::transfer_queue::{TransferKind, TransferQueue};
    use tuki_core::port_errors::TransferError;
    use tuki_core::ports::{ConnectionId, MockFileTransfer};
    use tuki_core::remote_path::RemotePath;

    use super::*;

    fn block_on<F: Future>(future: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(future)
    }

    fn download(name: &str) -> TransferKind {
        TransferKind::Download {
            remote: RemotePath::new(&format!("/srv/{name}")).unwrap(),
            local: Path::new("downloads").join(name),
        }
    }

    #[derive(Clone, Default)]
    struct RecordingEvents {
        batches: Arc<Mutex<Vec<Vec<TransferDto>>>>,
    }

    impl TransferEvents for RecordingEvents {
        fn emit(&self, jobs: &[TransferDto]) {
            self.batches.lock().unwrap().push(jobs.to_vec());
        }
    }

    impl RecordingEvents {
        fn state_of(&self, id: u64) -> Option<&'static str> {
            self.batches
                .lock()
                .unwrap()
                .iter()
                .rev()
                .flat_map(|batch| batch.iter())
                .find(|dto| dto.id == id)
                .map(|dto| dto.state)
        }
    }

    async fn wait_for_state(events: &RecordingEvents, id: u64, target: &'static str) {
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if events.state_of(id) == Some(target) {
                    return;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("timeout esperando el estado de la transferencia");
    }

    #[test]
    fn should_map_every_transfer_state_when_serializing_dtos() {
        let mut queue = TransferQueue::default();
        let id = queue.enqueue(ConnectionId::new(4), download("a.txt"));
        assert_eq!(to_dto(queue.jobs().first().unwrap()).state, "queued");

        queue.start_next();
        queue.progress(id, 5, Some(10));
        let dto = to_dto(queue.jobs().first().unwrap());
        assert_eq!(
            (dto.state, dto.transferred, dto.total),
            ("running", 5, Some(10))
        );

        queue.finish(id, Ok(10));
        let dto = to_dto(queue.jobs().first().unwrap());
        assert_eq!(
            (dto.state, dto.transferred, dto.total),
            ("done", 10, Some(10))
        );

        let failed = queue.enqueue(ConnectionId::new(4), download("b.txt"));
        queue.start_next();
        queue.finish(failed, Err(TransferError::Remote));
        let dto = to_dto(queue.jobs().get(1).unwrap());
        assert_eq!(
            (dto.state, dto.error.as_deref()),
            ("failed", Some("remote transfer failed"))
        );

        let cancelled = queue.enqueue(ConnectionId::new(4), download("c.txt"));
        queue.cancel(cancelled);
        assert_eq!(to_dto(queue.jobs().get(2).unwrap()).state, "cancelled");

        let uploaded = queue.enqueue(
            ConnectionId::new(4),
            TransferKind::Upload {
                local: Path::new("uploads/d.txt").to_path_buf(),
                remote: RemotePath::new("/srv/d.txt").unwrap(),
            },
        );
        assert_eq!(to_dto(queue.jobs().get(3).unwrap()).direction, "upload");
        assert_eq!(uploaded.get(), 3);

        let json = serde_json::to_string(&to_dto(queue.jobs().first().unwrap())).unwrap();
        assert!(json.contains("\"connectionId\":4"));
        assert!(json.contains("\"direction\":\"download\""));
        assert!(json.contains("\"transferred\":10"));
    }

    #[test]
    fn should_emit_first_progress_and_throttle_rapid_updates() {
        let now = std::time::Instant::now() + Duration::from_secs(10);
        assert!(Throttle::should_emit(now - Duration::from_millis(101), now));
        assert!(Throttle::should_emit(now - Duration::from_millis(100), now));
        assert!(!Throttle::should_emit(now - Duration::from_millis(99), now));
    }

    #[test]
    fn should_reject_missing_or_non_directory_local_dir_when_validating_download() {
        assert_eq!(
            block_on(ensure_local_dir("missing-transfer-dir"))
                .unwrap_err()
                .kind,
            "invalid_input"
        );
        let file = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        assert_eq!(
            block_on(ensure_local_dir(file.to_str().unwrap()))
                .unwrap_err()
                .kind,
            "invalid_input"
        );
        assert!(block_on(ensure_local_dir(env!("CARGO_MANIFEST_DIR"))).is_ok());
    }

    #[test]
    fn should_reject_invalid_names_when_building_transfer_targets() {
        for name in ["", ".", "..", "bad/name", "bad\\name"] {
            assert_eq!(
                remote_upload_path("/srv", name).unwrap_err().kind,
                "invalid_input",
                "{name}"
            );
        }
        assert_eq!(
            upload_target("..", "/srv").unwrap_err().kind,
            "invalid_input"
        );
        let (local, remote) = upload_target("uploads/report.txt", "/srv").unwrap();
        assert_eq!(local, Path::new("uploads/report.txt"));
        assert_eq!(remote.as_str(), "/srv/report.txt");

        let (remote, local) = download_target("/srv/a.txt", "downloads").unwrap();
        assert_eq!(remote.as_str(), "/srv/a.txt");
        assert_eq!(local, Path::new("downloads/a.txt"));
    }

    #[test]
    fn should_reject_missing_local_file_when_validating_upload() {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        assert!(block_on(ensure_local_file(manifest.to_str().unwrap())).is_ok());
        assert_eq!(
            block_on(ensure_local_file(env!("CARGO_MANIFEST_DIR")))
                .unwrap_err()
                .kind,
            "invalid_input"
        );
        assert_eq!(
            block_on(ensure_local_file("missing-transfer-file"))
                .unwrap_err()
                .kind,
            "invalid_input"
        );
    }

    #[test]
    fn should_process_queued_transfers_one_at_a_time_when_worker_runs() {
        let queue = Arc::new(Mutex::new(TransferQueue::default()));
        let wake = Arc::new(tokio::sync::Notify::new());
        let events = RecordingEvents::default();
        for name in ["a.txt", "b.txt"] {
            queue
                .lock()
                .unwrap()
                .enqueue(ConnectionId::new(1), download(name));
        }

        let active = Arc::new(AtomicUsize::new(0));
        let order = Arc::new(Mutex::new(Vec::new()));
        let mut fs = MockFileTransfer::new();
        fs.expect_upload().never();
        for name in ["a.txt", "b.txt"] {
            let name = name.to_owned();
            let active = Arc::clone(&active);
            let order = Arc::clone(&order);
            fs.expect_download()
                .times(1)
                .returning(move |_, remote, _, _, _| {
                    assert_eq!(remote.as_str().rsplit('/').next(), Some(name.as_str()));
                    assert_eq!(active.fetch_add(1, Ordering::SeqCst), 0);
                    order.lock().unwrap().push(name.clone());
                    let active = Arc::clone(&active);
                    Box::pin(async move {
                        tokio::task::yield_now().await;
                        active.fetch_sub(1, Ordering::SeqCst);
                        Ok(3)
                    })
                });
        }

        block_on(async {
            let handle = tokio::spawn(transfer_worker(
                Arc::clone(&queue),
                Arc::clone(&wake),
                fs,
                Arc::new(events.clone()),
            ));
            wait_for_state(&events, 0, "done").await;
            wait_for_state(&events, 1, "done").await;
            handle.abort();
        });

        assert_eq!(*order.lock().unwrap(), vec!["a.txt", "b.txt"]);
        assert_eq!(active.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn should_finish_as_cancelled_when_running_transfer_is_cancelled() {
        let queue = Arc::new(Mutex::new(TransferQueue::default()));
        let wake = Arc::new(tokio::sync::Notify::new());
        let events = RecordingEvents::default();
        let id = queue
            .lock()
            .unwrap()
            .enqueue(ConnectionId::new(2), download("c.txt"));

        let mut fs = MockFileTransfer::new();
        fs.expect_upload().never();
        fs.expect_download()
            .times(1)
            .returning(move |_, _, _, _, cancel| {
                let token = cancel.clone();
                Box::pin(async move {
                    while !token.is_cancelled() {
                        tokio::task::yield_now().await;
                    }
                    Err(TransferError::Cancelled)
                })
            });

        block_on(async {
            let handle = tokio::spawn(transfer_worker(
                Arc::clone(&queue),
                Arc::clone(&wake),
                fs,
                Arc::new(events.clone()),
            ));
            wait_for_state(&events, id.get(), "running").await;
            assert!(queue.lock().unwrap().cancel(id));
            wait_for_state(&events, id.get(), "cancelled").await;
            handle.abort();
        });
    }
}
