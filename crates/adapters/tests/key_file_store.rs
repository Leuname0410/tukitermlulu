//! Integración: importación de llaves al directorio privado con permisos restringidos.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use tempfile::tempdir;
use tuki_adapters::key_file_store::FsKeyFileStore;
use tuki_core::auth::KeyRef;
use tuki_core::port_errors::KeyError;
use tuki_core::ports::KeyFileStore;

fn key_ref(path: &Path) -> KeyRef {
    let path = path
        .to_str()
        .expect("ruta de llave válida en UTF-8");
    KeyRef::new(path).expect("ruta de llave no vacía")
}

async fn import_into(dir: &Path, source: &Path) -> Result<KeyRef, KeyError> {
    FsKeyFileStore::new(dir).import(&key_ref(source)).await
}

async fn write_source(name: &str, content: &[u8]) -> (tempfile::TempDir, PathBuf) {
    let source_dir = tempdir().expect("directorio de origen");
    let path = source_dir.path().join(name);
    std::fs::write(&path, content).expect("archivo de origen");
    (source_dir, path)
}

#[tokio::test]
async fn should_copy_bytes_and_return_key_ref_inside_dir_when_importing_key() {
    let dir = tempdir().expect("directorio temporal");
    let expected = b"-----BEGIN OPENSSH PRIVATE KEY-----\nabc\n" as &[u8];
    let (_source_dir, source) = write_source("id_ed25519", expected).await;

    let imported = import_into(&dir.path().join("private"), &source)
        .await
        .expect("importación de la llave");
    let path = PathBuf::from(imported.path());

    assert_eq!(path.parent(), Some(dir.path().join("private").as_path()));
    assert_eq!(path.file_name(), Some(OsStr::new("id_ed25519")));
    assert_eq!(
        tokio::fs::read(&path).await.expect("contenido importado"),
        expected
    );
}

#[cfg(unix)]
#[tokio::test]
async fn should_use_private_permissions_when_importing_key() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempdir().expect("directorio temporal");
    let private = dir.path().join("private");
    let (_source_dir, source) = write_source("id_ed25519", b"key").await;

    let imported = import_into(&private, &source)
        .await
        .expect("importación de la llave");

    let dir_mode = std::fs::metadata(&private)
        .expect("metadatos del directorio")
        .permissions()
        .mode();
    let file_mode = std::fs::metadata(imported.path())
        .expect("metadatos de la llave")
        .permissions()
        .mode();
    assert_eq!(dir_mode & 0o777, 0o700);
    assert_eq!(file_mode & 0o777, 0o600);
}

#[tokio::test]
async fn should_reuse_path_when_importing_the_same_key_twice() {
    let dir = tempdir().expect("directorio temporal");
    let (_source_dir, source) = write_source("id_ed25519", b"key").await;

    let first = import_into(dir.path(), &source)
        .await
        .expect("primera importación");
    let second = import_into(dir.path(), &source)
        .await
        .expect("segunda importación");

    assert_eq!(first, second);
    let entries = std::fs::read_dir(dir.path()).expect("contenido del directorio");
    assert_eq!(entries.count(), 1);
}

#[tokio::test]
async fn should_add_numeric_suffix_when_source_name_is_taken_by_other_content() {
    let dir = tempdir().expect("directorio temporal");
    let (_first_dir, first_source) = write_source("id_ed25519", b"first").await;
    let (_second_dir, second_source) = write_source("id_ed25519", b"second").await;
    let (_third_dir, third_source) = write_source("id_ed25519", b"third").await;

    let first = import_into(dir.path(), &first_source)
        .await
        .expect("primera importación");
    let second = import_into(dir.path(), &second_source)
        .await
        .expect("segunda importación");
    let third = import_into(dir.path(), &third_source)
        .await
        .expect("tercera importación");

    assert_eq!(
        Path::new(first.path()).file_name(),
        Some(OsStr::new("id_ed25519"))
    );
    assert_eq!(
        Path::new(second.path()).file_name(),
        Some(OsStr::new("id_ed25519-1"))
    );
    assert_eq!(
        Path::new(third.path()).file_name(),
        Some(OsStr::new("id_ed25519-2"))
    );
    assert_eq!(
        tokio::fs::read(first.path())
            .await
            .expect("contenido original"),
        b"first"
    );
    assert_eq!(
        tokio::fs::read(second.path()).await.expect("contenido -1"),
        b"second"
    );
}

#[tokio::test]
async fn should_return_not_found_when_source_does_not_exist() {
    let dir = tempdir().expect("directorio temporal");
    let missing = dir.path().join("missing_key");

    let error = import_into(dir.path(), &missing)
        .await
        .expect_err("origen inexistente");

    assert_eq!(error, KeyError::NotFound);
    assert_eq!(error.to_string(), "key file not found");
}

#[tokio::test]
async fn should_return_not_found_when_source_has_no_file_name() {
    let dir = tempdir().expect("directorio temporal");
    let without_name = dir.path().join("..");

    let error = import_into(dir.path(), &without_name)
        .await
        .expect_err("origen sin nombre de archivo");

    assert_eq!(error, KeyError::NotFound);
}
