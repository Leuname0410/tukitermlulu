//! Puertos del dominio y tipos de apoyo. Sin I/O.

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::Future;
    use std::sync::Arc;
    use std::task::{Context, Poll, Wake, Waker};

    struct Noop;

    impl Wake for Noop {
        fn wake(self: Arc<Self>) {}
    }

    fn block_on<F: Future>(future: F) -> F::Output {
        let waker = Waker::from(Arc::new(Noop));
        let mut context = Context::from_waker(&waker);
        let mut future = std::pin::pin!(future);
        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => output,
            Poll::Pending => panic!("future pending"),
        }
    }

    fn path() -> RemotePath {
        RemotePath::new("/tmp").unwrap()
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
}
