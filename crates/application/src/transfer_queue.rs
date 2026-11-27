//! Cola de transferencias y resolución segura de destinos locales.

use std::path::{Path, PathBuf};

use tuki_core::port_errors::TransferError;
use tuki_core::ports::{CancelToken, ConnectionId};
use tuki_core::remote_path::RemotePath;

/// Identificador opaco de una transferencia.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TransferId(u64);

impl TransferId {
    /// Crea un identificador de transferencia.
    pub fn new(v: u64) -> Self {
        Self(v)
    }

    /// Devuelve el valor numérico del identificador.
    pub fn get(self) -> u64 {
        self.0
    }
}

/// Transferencia encolada, con sus rutas de origen y destino.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferKind {
    Download { remote: RemotePath, local: PathBuf },
    Upload { local: PathBuf, remote: RemotePath },
}

/// Estado observable de una transferencia.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferState {
    Queued,
    Running {
        transferred: u64,
        total: Option<u64>,
    },
    Done {
        bytes: u64,
    },
    Failed(TransferError),
    Cancelled,
}

/// Trabajo de la cola compartido con el worker.
#[derive(Clone)]
pub struct TransferJob {
    pub id: TransferId,
    pub conn: ConnectionId,
    pub kind: TransferKind,
    pub state: TransferState,
    pub cancel: CancelToken,
}

impl std::fmt::Debug for TransferJob {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TransferJob")
            .field("id", &self.id)
            .field("conn", &self.conn)
            .field("kind", &self.kind)
            .field("state", &self.state)
            .field("cancel", &self.cancel.is_cancelled())
            .finish()
    }
}

/// Cola FIFO de transferencias; como máximo una está en ejecución.
#[derive(Default)]
pub struct TransferQueue {
    jobs: Vec<TransferJob>,
    next_id: u64,
}

impl TransferQueue {
    /// Añade un trabajo al final de la cola.
    pub fn enqueue(&mut self, conn: ConnectionId, kind: TransferKind) -> TransferId {
        let id = TransferId(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        self.jobs.push(TransferJob {
            id,
            conn,
            kind,
            state: TransferState::Queued,
            cancel: CancelToken::default(),
        });
        id
    }

    /// Pone en marcha el primer trabajo encolado si no hay otro en ejecución.
    pub fn start_next(&mut self) -> Option<TransferJob> {
        if self
            .jobs
            .iter()
            .any(|job| matches!(job.state, TransferState::Running { .. }))
        {
            return None;
        }
        let job = self
            .jobs
            .iter_mut()
            .find(|job| matches!(job.state, TransferState::Queued))?;
        job.state = TransferState::Running {
            transferred: 0,
            total: None,
        };
        Some(job.clone())
    }

    /// Actualiza el progreso únicamente del trabajo indicado si está en marcha.
    pub fn progress(&mut self, id: TransferId, transferred: u64, total: Option<u64>) {
        if let Some(job) = self
            .jobs
            .iter_mut()
            .find(|job| job.id == id && matches!(job.state, TransferState::Running { .. }))
        {
            job.state = TransferState::Running { transferred, total };
        }
    }

    /// Registra el resultado final del trabajo indicado.
    pub fn finish(&mut self, id: TransferId, result: Result<u64, TransferError>) {
        let state = match result {
            Ok(bytes) => TransferState::Done { bytes },
            Err(TransferError::Cancelled) => TransferState::Cancelled,
            Err(error) => TransferState::Failed(error),
        };
        if let Some(job) = self.jobs.iter_mut().find(|job| job.id == id) {
            job.state = state;
        }
    }

    /// Cancela un trabajo encolado o señala la cancelación al worker.
    pub fn cancel(&mut self, id: TransferId) -> bool {
        self.jobs
            .iter_mut()
            .find(|job| job.id == id)
            .is_some_and(Self::cancel_job)
    }

    /// Cancela los trabajos activos de una conexión al desconectar.
    pub fn cancel_connection(&mut self, conn: ConnectionId) {
        for job in self.jobs.iter_mut().filter(|job| job.conn == conn) {
            Self::cancel_job(job);
        }
    }

    /// Elimina los trabajos terminados, fallidos o cancelados.
    pub fn clear_finished(&mut self) {
        self.jobs.retain(|job| {
            matches!(
                job.state,
                TransferState::Queued | TransferState::Running { .. }
            )
        });
    }

    /// Devuelve todos los trabajos de la cola.
    pub fn jobs(&self) -> &[TransferJob] {
        &self.jobs
    }

    fn cancel_job(job: &mut TransferJob) -> bool {
        if matches!(job.state, TransferState::Queued) {
            job.state = TransferState::Cancelled;
            true
        } else if matches!(job.state, TransferState::Running { .. }) {
            job.cancel.cancel();
            true
        } else {
            false
        }
    }
}

/// Ruta local segura para descargar `remote` dentro de `dir`.
pub fn local_download_target(dir: &Path, remote: &RemotePath) -> Result<PathBuf, TransferError> {
    let name = remote.as_str().rsplit('/').next().unwrap_or("");
    if name.is_empty() || name == "." || name == ".." {
        return Err(TransferError::LocalIo);
    }
    if name.chars().any(|c| matches!(c, '/' | '\\' | ':' | '\0')) {
        return Err(TransferError::LocalIo);
    }
    let name = name.trim_end_matches([' ', '.']);
    if name.is_empty() {
        return Err(TransferError::LocalIo);
    }
    let stem = name.split('.').next().unwrap_or("");
    let upper = stem.to_ascii_uppercase();
    let numbered = upper
        .strip_prefix("COM")
        .or_else(|| upper.strip_prefix("LPT"));
    let reserved = matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || matches!(
            numbered,
            Some("1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
        );
    if reserved {
        return Err(TransferError::LocalIo);
    }
    Ok(dir.join(name))
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use tuki_core::port_errors::TransferError;
    use tuki_core::ports::ConnectionId;
    use tuki_core::remote_path::RemotePath;

    use super::{TransferId, TransferKind, TransferQueue, TransferState, local_download_target};

    fn download(name: &str) -> TransferKind {
        TransferKind::Download {
            remote: RemotePath::new(&format!("/srv/{name}")).expect("ruta válida"),
            local: PathBuf::from(name),
        }
    }

    fn path(value: &str) -> RemotePath {
        RemotePath::new(value).expect("ruta válida")
    }

    fn enqueue(queue: &mut TransferQueue) -> TransferId {
        queue.enqueue(ConnectionId::new(1), download("a.txt"))
    }

    #[test]
    fn should_start_jobs_in_fifo_order_when_one_job_finishes() {
        let mut queue = TransferQueue::default();
        let first = enqueue(&mut queue);
        let second = enqueue(&mut queue);
        let third = enqueue(&mut queue);
        assert_eq!((first.get(), second.get(), third.get()), (0, 1, 2));
        let started = queue.start_next().expect("trabajo encolado");
        assert_eq!(started.id, first);
        assert_eq!(
            started.state,
            TransferState::Running {
                transferred: 0,
                total: None
            }
        );
        assert!(queue.start_next().is_none());
        queue.finish(first, Ok(10));
        assert_eq!(queue.start_next().map(|job| job.id), Some(second));
        queue.finish(second, Ok(1));
        assert_eq!(queue.start_next().map(|job| job.id), Some(third));
    }

    #[test]
    fn should_record_result_when_finishing() {
        let mut queue = TransferQueue::default();
        for (result, expected) in [
            (Ok(42), TransferState::Done { bytes: 42 }),
            (Err(TransferError::Cancelled), TransferState::Cancelled),
            (
                Err(TransferError::Remote),
                TransferState::Failed(TransferError::Remote),
            ),
        ] {
            let id = enqueue(&mut queue);
            queue.start_next();
            queue.finish(id, result);
            assert_eq!(queue.jobs().last().expect("trabajo").state, expected);
        }
    }

    #[test]
    fn should_skip_cancelled_jobs_when_starting_next() {
        let mut queue = TransferQueue::default();
        let cancelled = enqueue(&mut queue);
        let pending = enqueue(&mut queue);
        assert!(queue.cancel(cancelled));
        assert_eq!(queue.jobs()[0].state, TransferState::Cancelled);
        assert_eq!(queue.start_next().map(|job| job.id), Some(pending));
        queue.finish(pending, Ok(1));
        assert!(queue.start_next().is_none());
    }

    #[test]
    fn should_signal_cancellation_when_cancelling_a_running_job() {
        let mut queue = TransferQueue::default();
        let id = enqueue(&mut queue);
        let started = queue.start_next().expect("trabajo encolado");
        assert!(queue.cancel(id));
        assert!(started.cancel.is_cancelled());
        assert!(queue.jobs()[0].cancel.is_cancelled());
        assert!(matches!(
            queue.jobs()[0].state,
            TransferState::Running { .. }
        ));
        queue.finish(id, Ok(0));
        assert!(!queue.cancel(id));
        assert!(!queue.cancel(TransferId::new(99)));
    }

    #[test]
    fn should_cancel_only_jobs_of_connection_when_disconnecting() {
        let mut queue = TransferQueue::default();
        let conn = ConnectionId::new(1);
        let first = queue.enqueue(conn, download("a.txt"));
        queue.enqueue(conn, download("b.txt"));
        let other = queue.enqueue(ConnectionId::new(2), download("c.txt"));
        queue.start_next();
        queue.cancel_connection(conn);
        assert!(queue.jobs()[0].cancel.is_cancelled());
        assert_eq!(queue.jobs()[1].state, TransferState::Cancelled);
        assert!(matches!(queue.jobs()[2].state, TransferState::Queued));
        queue.finish(first, Ok(0));
        assert_eq!(queue.start_next().map(|job| job.id), Some(other));
    }

    #[test]
    fn should_update_only_running_job_when_reporting_progress() {
        let mut queue = TransferQueue::default();
        let running = enqueue(&mut queue);
        let queued = enqueue(&mut queue);
        queue.start_next();
        queue.progress(queued, 50, Some(100));
        assert!(matches!(queue.jobs()[1].state, TransferState::Queued));
        queue.progress(running, 25, Some(100));
        assert!(matches!(
            queue.jobs()[0].state,
            TransferState::Running {
                transferred: 25,
                total: Some(100)
            }
        ));
    }

    #[test]
    fn should_keep_active_jobs_when_clearing_finished() {
        let mut queue = TransferQueue::default();
        let done = enqueue(&mut queue);
        queue.start_next();
        queue.finish(done, Ok(1));
        let failed = enqueue(&mut queue);
        queue.start_next();
        queue.finish(failed, Err(TransferError::Remote));
        let cancelled = enqueue(&mut queue);
        queue.cancel(cancelled);
        let active = enqueue(&mut queue);
        queue.start_next();
        queue.clear_finished();
        assert_eq!(queue.jobs().len(), 1);
        assert_eq!(queue.jobs()[0].id, active);
    }

    #[test]
    fn should_use_last_component_and_stay_inside_dir_when_resolving_target() {
        let dir = Path::new("downloads");
        for (remote, name) in [("/srv/a.txt", "a.txt"), ("/srv/../b.txt", "b.txt")] {
            let target = local_download_target(dir, &path(remote)).expect("nombre seguro");
            assert_eq!(target, dir.join(name));
            assert!(target.starts_with(dir));
        }
        let target = local_download_target(dir, &path("/srv/report. ")).expect("nombre seguro");
        assert_eq!(target, dir.join("report"));
    }

    #[test]
    fn should_reject_unsafe_names_when_resolving_download_target() {
        let dir = Path::new("downloads");
        for value in [
            "..", "a\\b", "C:x", ".", "/srv/", "CON", "con.txt", "nul ", "com1", "LPT9", "AUX.txt",
            "PRN.log",
        ] {
            assert_eq!(
                local_download_target(dir, &path(value)),
                Err(TransferError::LocalIo),
                "{value}"
            );
        }
    }

    #[test]
    fn should_allow_non_reserved_names_when_resolving_download_target() {
        let dir = Path::new("downloads");
        for value in ["console.txt", "com0.txt", "reporte"] {
            assert!(local_download_target(dir, &path(value)).is_ok());
        }
    }
}
