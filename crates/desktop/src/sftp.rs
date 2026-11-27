//! Puente IPC para la navegación SFTP sobre la conexión de la pestaña.

#[cfg(test)]
mod tests {
    use std::future::Future;

    use tuki_core::port_errors::FsError;
    use tuki_core::ports::{ConnectionId, FileEntry, MockRemoteFileSystem, RemoteFileSystem};
    use tuki_core::remote_path::RemotePath;

    use super::*;

    fn block_on<F: Future>(future: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(future)
    }

    fn entry(path: &str, is_dir: bool, size: u64) -> FileEntry {
        FileEntry {
            path: RemotePath::new(path).unwrap(),
            is_dir,
            size,
        }
    }

    #[test]
    fn should_sort_directories_first_and_by_name_when_listing_entries() {
        let mut fs = MockRemoteFileSystem::new();
        fs.expect_list().once().returning(|_, _| {
            Box::pin(async {
                Ok(vec![
                    entry("/srv/z.txt", false, 3),
                    entry("/srv/b", true, 0),
                    entry("/srv/a.txt", false, 7),
                    entry("/srv/a", true, 0),
                ])
            })
        });

        let dtos = block_on(list(&fs, ConnectionId::new(4), "/srv")).unwrap();
        let names: Vec<&str> = dtos.iter().map(|dto| dto.name.as_str()).collect();
        assert_eq!(names, vec!["a", "b", "a.txt", "z.txt"]);
        assert_eq!(dtos[0].path, "/srv/a");
        assert_eq!(dtos[0].size, 0);
        let json = serde_json::to_string(&dtos[0]).unwrap();
        assert!(json.contains("\"name\":\"a\""));
        assert!(json.contains("\"isDir\":true"));
        assert!(json.contains("\"size\":0"));
    }

    #[test]
    fn should_map_every_fs_error_when_an_operation_fails() {
        assert_eq!(from_fs_error(FsError::NotFound).kind(), "not_found");
        assert_eq!(
            from_fs_error(FsError::PermissionDenied).kind(),
            "permission_denied"
        );
        assert_eq!(from_fs_error(FsError::Operation).kind(), "io");
    }

    #[test]
    fn should_propagate_fs_errors_when_running_sftp_commands() {
        let conn = ConnectionId::new(4);
        let mut fs = MockRemoteFileSystem::new();
        fs.expect_home()
            .once()
            .returning(|_| Box::pin(async { Err(FsError::PermissionDenied) }));
        fs.expect_list()
            .once()
            .returning(|_, _| Box::pin(async { Err(FsError::NotFound) }));
        fs.expect_mkdir()
            .once()
            .returning(|_, _| Box::pin(async { Err(FsError::Operation) }));
        fs.expect_rename()
            .once()
            .returning(|_, _, _| Box::pin(async { Err(FsError::PermissionDenied) }));
        fs.expect_remove()
            .once()
            .returning(|_, _| Box::pin(async { Err(FsError::NotFound) }));

        assert_eq!(
            block_on(home(&fs, conn)).unwrap_err().kind(),
            "permission_denied"
        );
        assert_eq!(block_on(list(&fs, conn, "/srv")).unwrap_err().kind(), "not_found");
        assert_eq!(
            block_on(mkdir(&fs, conn, "/srv/new"))
                .unwrap_err()
                .kind(),
            "io"
        );
        assert_eq!(
            block_on(rename(&fs, conn, "/srv/a", "/srv/b"))
                .unwrap_err()
                .kind(),
            "permission_denied"
        );
        assert_eq!(
            block_on(remove(&fs, conn, "/srv/a")).unwrap_err().kind(),
            "not_found"
        );
    }

    #[test]
    fn should_reject_invalid_paths_when_commands_receive_them() {
        let conn = ConnectionId::new(4);
        let fs = MockRemoteFileSystem::new();
        assert_eq!(
            block_on(list(&fs, conn, "")).unwrap_err().kind(),
            "invalid_input"
        );
        assert_eq!(
            block_on(mkdir(&fs, conn, "")).unwrap_err().kind(),
            "invalid_input"
        );
        assert_eq!(
            block_on(rename(&fs, conn, "", "/to")).unwrap_err().kind(),
            "invalid_input"
        );
        assert_eq!(
            block_on(rename(&fs, conn, "/from", ""))
                .unwrap_err()
                .kind(),
            "invalid_input"
        );
        assert_eq!(
            block_on(remove(&fs, conn, "bad\0path"))
                .unwrap_err()
                .kind(),
            "invalid_input"
        );
    }
}
