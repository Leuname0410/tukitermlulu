//! Almacén de archivos de llave en el directorio privado de la aplicación.
//!
//! En Unix el directorio se crea con `0700` y cada archivo con `0600`. En Windows se confía
//! en la ACL heredada del directorio elegido por la composition root (`%APPDATA%`).

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use tuki_core::auth::KeyRef;
use tuki_core::port_errors::KeyError;
use tuki_core::ports::KeyFileStore;
use zeroize::Zeroizing;

/// Número máximo de intentos al elegir un nombre de archivo temporal único.
const MAX_TEMP_ATTEMPTS: u64 = 1024;
/// Contador de nombres temporales dentro del proceso.
static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

/// Copia llaves al directorio privado sin sobrescribir contenido distinto.
pub struct FsKeyFileStore {
    dir: PathBuf,
}

impl FsKeyFileStore {
    /// Crea el almacén sobre `dir`, que se creará con permisos privados si no existe.
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }
}

impl KeyFileStore for FsKeyFileStore {
    async fn import(&self, source: &KeyRef) -> Result<KeyRef, KeyError> {
        let source_path = PathBuf::from(source.path());
        let name = source_path
            .file_name()
            .ok_or(KeyError::NotFound)?
            .to_os_string();
        let content =
            Zeroizing::new(tokio::fs::read(&source_path).await.map_err(
                |error| match error.kind() {
                    std::io::ErrorKind::NotFound => KeyError::NotFound,
                    _ => KeyError::Io,
                },
            )?);

        let mut builder = tokio::fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        builder.mode(0o700);
        builder.create(&self.dir).await.map_err(|_| KeyError::Io)?;

        for suffix in 0u64.. {
            let candidate = self.dir.join(versioned_name(&name, suffix));
            match tokio::fs::read(&candidate).await.map(Zeroizing::new) {
                Ok(existing) if existing.as_slice() == content.as_slice() => {
                    return to_key_ref(&candidate);
                }
                Ok(_) => continue,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    let temp = write_temp(&self.dir, &name, content.as_slice()).await?;
                    if tokio::fs::rename(&temp, &candidate).await.is_err() {
                        let _ = tokio::fs::remove_file(&temp).await;
                        return Err(KeyError::Io);
                    }
                    return to_key_ref(&candidate);
                }
                Err(_) => return Err(KeyError::Io),
            }
        }

        Err(KeyError::Io)
    }
}

/// Devuelve el nombre original o el nombre con sufijo `-N`.
fn versioned_name(name: &OsStr, suffix: u64) -> OsString {
    if suffix == 0 {
        return name.to_os_string();
    }
    let mut versioned = name.to_os_string();
    versioned.push(format!("-{suffix}"));
    versioned
}

/// Escribe un archivo temporal con permisos privados antes de renombrarlo.
async fn write_temp(dir: &Path, name: &OsStr, content: &[u8]) -> Result<PathBuf, KeyError> {
    for _ in 0..MAX_TEMP_ATTEMPTS {
        let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
        let mut temp_name = OsString::from(".tuki-key-");
        temp_name.push(name);
        temp_name.push(format!("-{}-{id}.tmp", std::process::id()));
        let temp = dir.join(temp_name);

        let mut options = tokio::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);
        match options.open(&temp).await {
            Ok(_) => {
                if tokio::fs::write(&temp, content).await.is_err() {
                    let _ = tokio::fs::remove_file(&temp).await;
                    return Err(KeyError::Io);
                }
                return Ok(temp);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return Err(KeyError::Io),
        }
    }

    Err(KeyError::Io)
}

/// Construye la referencia sin exponer rutas en los errores.
fn to_key_ref(path: &Path) -> Result<KeyRef, KeyError> {
    let path = path.to_str().ok_or(KeyError::Io)?;
    KeyRef::new(path).map_err(|_| KeyError::Io)
}
