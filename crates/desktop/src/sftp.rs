//! Puente IPC para la navegación SFTP sobre la conexión de la pestaña.

use serde::Serialize;
use tauri::State;
use tuki_core::port_errors::FsError;
use tuki_core::ports::{ConnectionId, FileEntry, RemoteFileSystem};
use tuki_core::remote_path::RemotePath;

use crate::terminal::{AppState, CommandError};

type Res<T> = Result<T, CommandError>;

/// Entrada de un directorio remoto, con nombre derivado de la ruta.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryDto {
    name: String,
    path: String,
    is_dir: bool,
    size: u64,
}

/// Traduce los fallos del sistema de archivos a categorías de error IPC.
fn from_fs_error(error: FsError) -> CommandError {
    let kind = match &error {
        FsError::NotFound => "not_found",
        FsError::PermissionDenied => "permission_denied",
        FsError::Operation => "io",
    };
    CommandError::new(kind, error)
}

/// Extrae el nombre de la última componente de la ruta remota.
fn entry_name(entry: &FileEntry) -> &str {
    entry.path.as_str().rsplit('/').next().unwrap_or_default()
}

/// Proyecta una entrada del dominio al DTO de la UI.
fn to_dto(entry: &FileEntry) -> EntryDto {
    EntryDto {
        name: entry_name(entry).to_owned(),
        path: entry.path.as_str().to_owned(),
        is_dir: entry.is_dir,
        size: entry.size,
    }
}

/// Devuelve el directorio inicial (`realpath(".")`) de la conexión.
async fn home<F: RemoteFileSystem>(fs: &F, id: ConnectionId) -> Res<String> {
    let path = fs.home(id).await.map_err(from_fs_error)?;
    Ok(path.as_str().to_owned())
}

/// Lista un directorio con las carpetas primero y por nombre dentro de cada grupo.
async fn list<F: RemoteFileSystem>(fs: &F, id: ConnectionId, path: &str) -> Res<Vec<EntryDto>> {
    let path = RemotePath::new(path).map_err(CommandError::invalid_input)?;
    let mut entries = fs.list(id, &path).await.map_err(from_fs_error)?;
    entries.sort_by(|left, right| {
        right
            .is_dir
            .cmp(&left.is_dir)
            .then_with(|| entry_name(left).cmp(entry_name(right)))
    });
    Ok(entries.iter().map(to_dto).collect())
}

/// Crea un directorio remoto.
async fn mkdir<F: RemoteFileSystem>(fs: &F, id: ConnectionId, path: &str) -> Res<()> {
    let path = RemotePath::new(path).map_err(CommandError::invalid_input)?;
    fs.mkdir(id, &path).await.map_err(from_fs_error)
}

/// Renombra una entrada remota.
async fn rename<F: RemoteFileSystem>(fs: &F, id: ConnectionId, from: &str, to: &str) -> Res<()> {
    let from = RemotePath::new(from).map_err(CommandError::invalid_input)?;
    let to = RemotePath::new(to).map_err(CommandError::invalid_input)?;
    fs.rename(id, &from, &to).await.map_err(from_fs_error)
}

/// Elimina un archivo o directorio remoto.
async fn remove<F: RemoteFileSystem>(fs: &F, id: ConnectionId, path: &str) -> Res<()> {
    let path = RemotePath::new(path).map_err(CommandError::invalid_input)?;
    fs.remove(id, &path).await.map_err(from_fs_error)
}

/// Directorio inicial de SFTP de la pestaña indicada.
#[tauri::command]
pub async fn sftp_home(state: State<'_, AppState>, connection_id: u64) -> Res<String> {
    home(&state.fs, ConnectionId::new(connection_id)).await
}

/// Listado ordenado del directorio remoto indicado.
#[tauri::command]
pub async fn sftp_list(
    state: State<'_, AppState>,
    connection_id: u64,
    path: String,
) -> Res<Vec<EntryDto>> {
    list(&state.fs, ConnectionId::new(connection_id), &path).await
}

/// Crea un directorio en la conexión indicada.
#[tauri::command]
pub async fn sftp_mkdir(state: State<'_, AppState>, connection_id: u64, path: String) -> Res<()> {
    mkdir(&state.fs, ConnectionId::new(connection_id), &path).await
}

/// Renombra una entrada en la conexión indicada.
#[tauri::command]
pub async fn sftp_rename(
    state: State<'_, AppState>,
    connection_id: u64,
    from: String,
    to: String,
) -> Res<()> {
    rename(&state.fs, ConnectionId::new(connection_id), &from, &to).await
}

/// Elimina una entrada en la conexión indicada.
#[tauri::command]
pub async fn sftp_remove(state: State<'_, AppState>, connection_id: u64, path: String) -> Res<()> {
    remove(&state.fs, ConnectionId::new(connection_id), &path).await
}

#[cfg(test)]
mod tests {
    use std::fmt::Debug;
    use std::future::Future;

    use tuki_core::port_errors::FsError;
    use tuki_core::ports::{ConnectionId, FileEntry, MockRemoteFileSystem};
    use tuki_core::remote_path::RemotePath;

    use super::*;

    fn block_on<F: Future>(future: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(future)
    }

    fn kind<T: Debug>(result: Res<T>) -> &'static str {
        result.unwrap_err().kind()
    }

    fn entry(path: &str, is_dir: bool, size: u64) -> FileEntry {
        FileEntry {
            path: RemotePath::new(path).unwrap(),
            is_dir,
            size,
            permissions: Some(0o640),
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
        assert_eq!(
            (dtos[0].path.as_str(), dtos[0].is_dir, dtos[0].size),
            ("/srv/a", true, 0)
        );
        let json = serde_json::to_string(&dtos[0]).unwrap();
        assert!(json.contains("\"isDir\":true"));
        assert!(json.contains("\"size\":0"));
        assert!(json.contains("\"permissions\":416"));
    }

    #[test]
    fn should_reject_out_of_range_mode_when_chmodding() {
        let fs = MockRemoteFileSystem::new();
        let id = ConnectionId::new(4);
        assert_eq!(
            kind(block_on(chmod(&fs, id, "/srv/a", 0o10000))),
            "invalid_input"
        );
        assert_eq!(
            kind(block_on(chmod(&fs, id, "/srv/a", 0o7777 + 1))),
            "invalid_input"
        );
        assert_eq!(
            kind(block_on(chmod(&fs, id, "", 0o640))),
            "invalid_input"
        );
    }

    #[test]
    fn should_forward_valid_mode_when_chmodding() {
        let mut fs = MockRemoteFileSystem::new();
        fs.expect_chmod()
            .once()
            .withf(|_, path, mode| path.as_str() == "/srv/a" && *mode == 0o640)
            .returning(|_, _, _| Box::pin(async { Ok(()) }));

        assert!(block_on(chmod(&fs, ConnectionId::new(4), "/srv/a", 0o640)).is_ok());
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
    fn should_propagate_fs_error_when_listing_fails() {
        let mut fs = MockRemoteFileSystem::new();
        fs.expect_list()
            .once()
            .returning(|_, _| Box::pin(async { Err(FsError::NotFound) }));
        assert_eq!(
            kind(block_on(list(&fs, ConnectionId::new(4), "/srv"))),
            "not_found"
        );
    }

    #[test]
    fn should_reject_invalid_paths_when_commands_receive_them() {
        let id = ConnectionId::new(4);
        let fs = MockRemoteFileSystem::new();
        assert_eq!(kind(block_on(list(&fs, id, ""))), "invalid_input");
        assert_eq!(kind(block_on(mkdir(&fs, id, ""))), "invalid_input");
        assert_eq!(kind(block_on(rename(&fs, id, "", "/to"))), "invalid_input");
        assert_eq!(
            kind(block_on(rename(&fs, id, "/from", ""))),
            "invalid_input"
        );
        assert_eq!(
            kind(block_on(remove(&fs, id, "bad\0path"))),
            "invalid_input"
        );
    }
}
