#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use tuki_core::port_errors::TransferError;
    use tuki_core::ports::ConnectionId;
    use tuki_core::remote_path::RemotePath;

    use super::{local_download_target, TransferId, TransferKind, TransferQueue, TransferState};

    fn download(name: &str) -> TransferKind {
        TransferKind::Download {
            remote: RemotePath::new(&format!("/srv/{name}")).expect("ruta válida"),
            local: PathBuf::from(name),
        }
    }

    fn remote(value: &str) -> RemotePath {
        RemotePath::new(value).expect("ruta válida")
    }

    #[test]
    fn should_start_jobs_in_fifo_order_when_one_job_finishes() {
        let mut queue = TransferQueue::default();
        let first = queue.enqueue(ConnectionId::new(1), download("a.txt"));
        let second = queue.enqueue(ConnectionId::new(1), download("b.txt"));
        let third = queue.enqueue(ConnectionId::new(1), download("c.txt"));

        assert_eq!(first, TransferId::new(0));
        assert_eq!(second.get(), 1);
        assert_eq!(third.get(), 2);

        let started = queue.start_next().expect("hay un trabajo encolado");
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
        let started = queue.start_next().expect("hay otro trabajo encolado");
        assert_eq!(started.id, second);
        queue.finish(second, Ok(1));
        let started = queue.start_next().expect("queda un trabajo encolado");
        assert_eq!(started.id, third);
    }

    #[test]
    fn should_mark_finished_with_done_when_finish_succeeds() {
        let mut queue = TransferQueue::default();
        let id = queue.enqueue(ConnectionId::new(1), download("a.txt"));
        queue.start_next();

        queue.finish(id, Ok(42));

        assert_eq!(queue.jobs()[0].state, TransferState::Done { bytes: 42 });
    }

    #[test]
    fn should_mark_finished_as_cancelled_when_finish_is_cancelled() {
        let mut queue = TransferQueue::default();
        let id = queue.enqueue(ConnectionId::new(1), download("a.txt"));
        queue.start_next();

        queue.finish(id, Err(TransferError::Cancelled));

        assert_eq!(queue.jobs()[0].state, TransferState::Cancelled);
    }

    #[test]
    fn should_mark_finished_as_failed_when_finish_returns_remote_error() {
        let mut queue = TransferQueue::default();
        let id = queue.enqueue(ConnectionId::new(1), download("a.txt"));
        queue.start_next();

        queue.finish(id, Err(TransferError::Remote));

        assert_eq!(
            queue.jobs()[0].state,
            TransferState::Failed(TransferError::Remote)
        );
    }

    #[test]
    fn should_skip_cancelled_jobs_when_starting_next() {
        let mut queue = TransferQueue::default();
        let cancelled = queue.enqueue(ConnectionId::new(1), download("a.txt"));
        let pending = queue.enqueue(ConnectionId::new(1), download("b.txt"));

        assert!(queue.cancel(cancelled));
        assert_eq!(queue.jobs()[0].state, TransferState::Cancelled);

        let started = queue.start_next().expect("queda un trabajo encolado");
        assert_eq!(started.id, pending);
        queue.finish(pending, Ok(1));
        assert!(queue.start_next().is_none());
    }

    #[test]
    fn should_signal_running_job_when_cancelling_it() {
        let mut queue = TransferQueue::default();
        let id = queue.enqueue(ConnectionId::new(1), download("a.txt"));
        let started = queue.start_next().expect("hay un trabajo encolado");

        assert!(queue.cancel(id));

        assert!(started.cancel.is_cancelled());
        assert!(queue.jobs()[0].cancel.is_cancelled());
        assert_eq!(
            queue.jobs()[0].state,
            TransferState::Running {
                transferred: 0,
                total: None
            }
        );
        queue.finish(id, Err(TransferError::Cancelled));
        assert_eq!(queue.jobs()[0].state, TransferState::Cancelled);
    }

    #[test]
    fn should_reject_cancel_when_job_is_finished_or_unknown() {
        let mut queue = TransferQueue::default();
        let id = queue.enqueue(ConnectionId::new(1), download("a.txt"));
        queue.finish(id, Ok(0));

        assert!(!queue.cancel(id));
        assert!(!queue.cancel(TransferId::new(99)));
    }

    #[test]
    fn should_cancel_only_jobs_of_connection_when_disconnecting() {
        let mut queue = TransferQueue::default();
        let conn = ConnectionId::new(1);
        let running = queue.enqueue(conn, download("a.txt"));
        let queued = queue.enqueue(conn, download("b.txt"));
        let other = queue.enqueue(ConnectionId::new(2), download("c.txt"));
        queue.start_next();
        assert_eq!(queue.jobs()[0].id, running);

        queue.cancel_connection(conn);

        assert!(queue.jobs()[0].cancel.is_cancelled());
        assert_eq!(queue.jobs()[1].state, TransferState::Cancelled);
        assert_eq!(queue.jobs()[2].state, TransferState::Queued);
        assert!(!queue.jobs()[2].cancel.is_cancelled());
        assert_eq!(queue.start_next().map(|job| job.id), Some(other));
    }

    #[test]
    fn should_update_only_running_job_when_reporting_progress() {
        let mut queue = TransferQueue::default();
        let running = queue.enqueue(ConnectionId::new(1), download("a.txt"));
        let queued = queue.enqueue(ConnectionId::new(1), download("b.txt"));
        queue.start_next();

        queue.progress(queued, 50, Some(100));
        assert_eq!(queue.jobs()[1].state, TransferState::Queued);

        queue.progress(running, 25, Some(100));
        assert_eq!(
            queue.jobs()[0].state,
            TransferState::Running {
                transferred: 25,
                total: Some(100)
            }
        );

        queue.progress(TransferId::new(99), 1, None);
        assert_eq!(
            queue.jobs()[0].state,
            TransferState::Running {
                transferred: 25,
                total: Some(100)
            }
        );
    }

    #[test]
    fn should_keep_active_jobs_when_clearing_finished() {
        let mut queue = TransferQueue::default();
        let done = queue.enqueue(ConnectionId::new(1), download("a.txt"));
        queue.start_next();
        queue.finish(done, Ok(1));
        let failed = queue.enqueue(ConnectionId::new(1), download("b.txt"));
        queue.start_next();
        queue.finish(failed, Err(TransferError::Remote));
        let cancelled = queue.enqueue(ConnectionId::new(1), download("c.txt"));
        queue.cancel(cancelled);
        let active = queue.enqueue(ConnectionId::new(1), download("d.txt"));
        queue.start_next();

        queue.clear_finished();

        assert_eq!(queue.jobs().len(), 1);
        assert_eq!(queue.jobs()[0].id, active);
    }

    #[test]
    fn should_use_last_remote_component_when_resolving_download_target() {
        let dir = Path::new("downloads");

        let target = local_download_target(dir, &remote("/srv/a.txt")).expect("nombre seguro");

        assert_eq!(target, dir.join("a.txt"));
        assert!(target.starts_with(dir));
        let target = local_download_target(dir, &remote("/srv/../b.txt")).expect("nombre seguro");
        assert_eq!(target, dir.join("b.txt"));
        assert!(target.starts_with(dir));
    }

    #[test]
    fn should_reject_unsafe_names_when_resolving_download_target() {
        let dir = Path::new("downloads");

        for value in ["..", "a\\b", "C:x", "CON", "con.txt", "nul ", "/srv/"] {
            assert_eq!(
                local_download_target(dir, &remote(value)),
                Err(TransferError::LocalIo),
                "entrada rechazada: {value}"
            );
        }
    }

    #[test]
    fn should_strip_trailing_spaces_and_dots_when_resolving_download_target() {
        let dir = Path::new("downloads");

        let target =
            local_download_target(dir, &remote("/srv/report. ")).expect("nombre seguro");
        assert_eq!(target, dir.join("report"));
        assert!(target.starts_with(dir));
    }

    #[test]
    fn should_reject_windows_reserved_names_when_resolving_download_target() {
        let dir = Path::new("downloads");

        for value in ["com1", "LPT9", "AUX.txt", "PRN.log"] {
            assert_eq!(
                local_download_target(dir, &remote(value)),
                Err(TransferError::LocalIo),
                "nombre reservado: {value}"
            );
        }
    }

    #[test]
    fn should_allow_non_reserved_names_when_resolving_download_target() {
        let dir = Path::new("downloads");

        for value in ["console.txt", "com0.txt", "reporte"] {
            let target = local_download_target(dir, &remote(value)).expect("nombre seguro");
            assert!(target.starts_with(dir));
        }
    }
}
