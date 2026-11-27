//! Pruebas del repositorio SQLite antes de la implementación (fase roja de TDD).

#[cfg(test)]
mod tests {
    use std::panic::{AssertUnwindSafe, catch_unwind};

    use tuki_core::auth::{AuthMethod, KeyRef};
    use tuki_core::host::{Host, Hostname, Port, Username};
    use tuki_core::port_errors::RepositoryError;
    use tuki_core::ports::SessionRepository;
    use tuki_core::session::{Folder, FolderId, Session, SessionId};

    use super::{MIGRATIONS, SqliteSessionRepository};

    fn host(port: u16, user: &str) -> Host {
        Host {
            hostname: Hostname::new("example.com").expect("host válido"),
            port: Port::new(port).expect("puerto válido"),
            username: Username::new(user).expect("usuario válido"),
        }
    }

    fn password_session(id: u64, name: &str, folder: Option<FolderId>) -> Session {
        Session::new(
            SessionId::new(id),
            name,
            host(22, "alice"),
            AuthMethod::Password { remember: true },
            folder,
        )
        .expect("sesión válida")
    }

    fn key_session(id: u64) -> Session {
        Session::new(
            SessionId::new(id),
            "keyed",
            host(2222, "bob"),
            AuthMethod::PublicKey {
                key: KeyRef::new("/home/bob/.ssh/id_ed25519").expect("ruta válida"),
                has_passphrase: true,
            },
            None,
        )
        .expect("sesión válida")
    }

    fn folder(id: u64, parent: Option<FolderId>) -> Folder {
        Folder::new(FolderId::new(id), &format!("folder-{id}"), parent).expect("carpeta válida")
    }

    #[tokio::test]
    async fn should_round_trip_password_session_when_saving_and_getting() {
        let repo = SqliteSessionRepository::in_memory().expect("repositorio");
        let session = password_session(1, "prod", None);
        repo.save_session(&session).await.expect("guardar");
        assert_eq!(
            repo.get_session(SessionId::new(1)).await.expect("leer"),
            Some(session)
        );
    }

    #[tokio::test]
    async fn should_round_trip_key_session_when_saving_and_getting() {
        let repo = SqliteSessionRepository::in_memory().expect("repositorio");
        let session = key_session(2);
        repo.save_session(&session).await.expect("guardar");
        assert_eq!(
            repo.get_session(SessionId::new(2)).await.expect("leer"),
            Some(session)
        );
    }

    #[tokio::test]
    async fn should_update_session_when_saving_same_id() {
        let repo = SqliteSessionRepository::in_memory().expect("repositorio");
        repo.save_session(&password_session(1, "prod", None))
            .await
            .expect("guardar");
        let updated = Session::new(
            SessionId::new(1),
            "staging",
            host(2222, "bob"),
            AuthMethod::PublicKey {
                key: KeyRef::new("/home/bob/.ssh/id_rsa").expect("ruta válida"),
                has_passphrase: false,
            },
            None,
        )
        .expect("sesión válida");
        repo.save_session(&updated).await.expect("actualizar");
        assert_eq!(
            repo.get_session(SessionId::new(1)).await.expect("leer"),
            Some(updated)
        );
        assert_eq!(repo.list_sessions().await.expect("listar").len(), 1);
    }

    #[tokio::test]
    async fn should_list_sessions_ordered_by_id_when_multiple_saved() {
        let repo = SqliteSessionRepository::in_memory().expect("repositorio");
        for id in [5, 2, 9] {
            repo.save_session(&password_session(id, "prod", None))
                .await
                .expect("guardar");
        }
        let ids: Vec<u64> = repo
            .list_sessions()
            .await
            .expect("listar")
            .iter()
            .map(|session| session.id.get())
            .collect();
        assert_eq!(ids, vec![2, 5, 9]);
    }

    #[tokio::test]
    async fn should_return_none_when_getting_deleted_session() {
        let repo = SqliteSessionRepository::in_memory().expect("repositorio");
        repo.save_session(&password_session(1, "prod", None))
            .await
            .expect("guardar");
        repo.delete_session(SessionId::new(1))
            .await
            .expect("borrar");
        assert_eq!(
            repo.get_session(SessionId::new(1)).await.expect("leer"),
            None
        );
    }

    #[tokio::test]
    async fn should_keep_hierarchy_when_saving_folders() {
        let repo = SqliteSessionRepository::in_memory().expect("repositorio");
        for (id, parent) in [(1, None), (2, Some(FolderId::new(1))), (3, None)] {
            repo.save_folder(&folder(id, parent))
                .await
                .expect("guardar");
        }
        assert_eq!(
            repo.list_folders().await.expect("listar"),
            vec![
                folder(1, None),
                folder(2, Some(FolderId::new(1))),
                folder(3, None)
            ]
        );
    }

    #[tokio::test]
    async fn should_cascade_delete_and_detach_sessions_when_deleting_parent_folder() {
        let repo = SqliteSessionRepository::in_memory().expect("repositorio");
        repo.save_folder(&folder(1, None)).await.expect("guardar");
        repo.save_folder(&folder(2, Some(FolderId::new(1))))
            .await
            .expect("guardar");
        repo.save_session(&password_session(1, "prod", Some(FolderId::new(2))))
            .await
            .expect("guardar");
        repo.delete_folder(FolderId::new(1)).await.expect("borrar");
        assert!(repo.list_folders().await.expect("listar").is_empty());
        let session = repo
            .get_session(SessionId::new(1))
            .await
            .expect("leer")
            .expect("sesión");
        assert_eq!(session.folder, None);
    }

    #[tokio::test]
    async fn should_return_one_when_asking_next_ids_on_empty_repository() {
        let repo = SqliteSessionRepository::in_memory().expect("repositorio");
        assert_eq!(repo.next_session_id().await.expect("id"), SessionId::new(1));
        assert_eq!(repo.next_folder_id().await.expect("id"), FolderId::new(1));
    }

    #[tokio::test]
    async fn should_return_max_plus_one_when_asking_next_ids_after_saving() {
        let repo = SqliteSessionRepository::in_memory().expect("repositorio");
        repo.save_session(&password_session(7, "prod", None))
            .await
            .expect("guardar");
        repo.save_folder(&folder(3, None)).await.expect("guardar");
        assert_eq!(repo.next_session_id().await.expect("id"), SessionId::new(8));
        assert_eq!(repo.next_folder_id().await.expect("id"), FolderId::new(4));
    }

    #[tokio::test]
    async fn should_return_not_found_when_deleting_unknown_entries() {
        let repo = SqliteSessionRepository::in_memory().expect("repositorio");
        assert_eq!(
            repo.delete_session(SessionId::new(99)).await,
            Err(RepositoryError::NotFound)
        );
        assert_eq!(
            repo.delete_folder(FolderId::new(99)).await,
            Err(RepositoryError::NotFound)
        );
    }

    #[tokio::test]
    async fn should_return_conflict_when_foreign_key_is_missing() {
        let repo = SqliteSessionRepository::in_memory().expect("repositorio");
        assert_eq!(
            repo.save_session(&password_session(1, "prod", Some(FolderId::new(99))))
                .await,
            Err(RepositoryError::Conflict)
        );
        assert_eq!(
            repo.save_folder(&folder(1, Some(FolderId::new(99))))
                .await,
            Err(RepositoryError::Conflict)
        );
    }

    #[tokio::test]
    async fn should_keep_data_and_bump_version_when_opening_same_file_twice() {
        let dir = tempfile::tempdir().expect("directorio temporal");
        let path = dir.path().join("sessions.db");
        let first = SqliteSessionRepository::open(&path).expect("abrir");
        first
            .save_session(&password_session(1, "prod", None))
            .await
            .expect("guardar");
        let second = SqliteSessionRepository::open(&path).expect("reabrir");
        assert!(path.exists());
        assert_eq!(
            second.get_session(SessionId::new(1)).await.expect("leer"),
            Some(password_session(1, "prod", None))
        );
        let version: u32 = second
            .conn
            .lock()
            .expect("cerrojo")
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("versión");
        assert_eq!(version as usize, MIGRATIONS.len());
    }

    #[tokio::test]
    async fn should_return_io_when_reading_corrupt_row() {
        let repo = SqliteSessionRepository::in_memory().expect("repositorio");
        repo.conn
            .lock()
            .expect("cerrojo")
            .execute(
                "INSERT INTO sessions (id, name, hostname, port, username, auth_kind, remember_password, key_path, key_has_passphrase, folder) \
                 VALUES (1, 'bad', '', 22, 'alice', 'password', 0, NULL, 0, NULL)",
                [],
            )
            .expect("fila corrupta");
        assert_eq!(repo.list_sessions().await, Err(RepositoryError::Io));
    }

    #[tokio::test]
    async fn should_have_no_secret_columns_when_inspecting_schema() {
        let repo = SqliteSessionRepository::in_memory().expect("repositorio");
        let conn = repo.conn.lock().expect("cerrojo");
        let mut stmt = conn.prepare("PRAGMA table_info(sessions)").expect("esquema");
        let rows = stmt.query_map([], |row| row.get::<_, String>(1)).expect("columnas");
        for name in rows {
            let name = name.expect("columna");
            assert!(!matches!(name.as_str(), "password" | "passphrase"));
        }
    }

    #[tokio::test]
    async fn should_return_io_when_mutex_is_poisoned() {
        let repo = SqliteSessionRepository::in_memory().expect("repositorio");
        let _ = catch_unwind(AssertUnwindSafe(|| {
            let _guard = repo.conn.lock().expect("cerrojo");
            panic!("envenenar");
        }));
        assert_eq!(repo.list_sessions().await, Err(RepositoryError::Io));
    }
}
