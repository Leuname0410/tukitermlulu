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
        assert_eq!((dto.state, dto.transferred, dto.total), ("running", 5, Some(10)));

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
            block_on(ensure_local_dir("missing-transfer-dir")).unwrap_err().kind,
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
            block_on(ensure_local_file("missing-transfer-file")).unwrap_err().kind,
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
            fs.expect_download().times(1).returning(move |_, remote, _, _, _| {
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
