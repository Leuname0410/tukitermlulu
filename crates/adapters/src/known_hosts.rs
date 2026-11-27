//! Verificación TOFU sobre un archivo known_hosts propiedad de la aplicación.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use tuki_core::host::Host;
use tuki_core::port_errors::HostKeyError;
use tuki_core::ports::{HostKeyFingerprint, HostKeyStatus, HostKeyVerifier};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Verificador persistente; la ruta del archivo la decide la composition root.
#[derive(Debug)]
pub struct FileHostKeyVerifier {
    path: PathBuf,
}

impl FileHostKeyVerifier {
    /// Crea el verificador sobre la ruta indicada.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Nombre del archivo temporal, único dentro del mismo directorio.
    fn temp_path(&self, parent: &Path) -> PathBuf {
        let name = self
            .path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("known_hosts");
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let unique = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        parent.join(format!(".{name}.{stamp}-{unique}.tmp"))
    }
}

/// Las huellas almacenadas deben tener el prefijo `SHA256:`.
fn validate(fingerprint: &HostKeyFingerprint) -> Result<(), HostKeyError> {
    if fingerprint.as_str().starts_with("SHA256:") {
        Ok(())
    } else {
        Err(HostKeyError::InvalidFingerprint)
    }
}

/// Clave de entrada: hostname en minúsculas y puerto entre corchetes.
fn entry_key(host: &Host) -> String {
    format!(
        "[{}]:{}",
        host.hostname.as_str().to_lowercase(),
        host.port.get()
    )
}

/// Primera palabra de una línea que no sea comentario.
fn line_key(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    trimmed.split_whitespace().next()
}

/// Entrada bien formada: exactamente la clave y la huella.
fn parse_entry(line: &str) -> Option<(&str, &str)> {
    let mut parts = line.split_whitespace();
    let key = parts.next()?;
    let fingerprint = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    Some((key, fingerprint))
}

impl HostKeyVerifier for FileHostKeyVerifier {
    async fn verify(
        &self,
        host: &Host,
        fingerprint: &HostKeyFingerprint,
    ) -> Result<HostKeyStatus, HostKeyError> {
        validate(fingerprint)?;
        let content = match tokio::fs::read_to_string(&self.path).await {
            Ok(content) => content,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(HostKeyStatus::Unknown),
            Err(_) => return Err(HostKeyError::Io),
        };

        let key = entry_key(host);
        let mut status = HostKeyStatus::Unknown;
        for line in content.lines() {
            match parse_entry(line) {
                Some((entry, stored)) if entry == key => {
                    status = if stored == fingerprint.as_str() {
                        HostKeyStatus::Known
                    } else {
                        HostKeyStatus::Changed
                    };
                }
                _ => {}
            }
        }
        Ok(status)
    }

    async fn trust(
        &self,
        host: &Host,
        fingerprint: &HostKeyFingerprint,
    ) -> Result<(), HostKeyError> {
        validate(fingerprint)?;
        let key = entry_key(host);
        let existing = match tokio::fs::read_to_string(&self.path).await {
            Ok(content) => content,
            Err(error) if error.kind() == ErrorKind::NotFound => String::new(),
            Err(_) => return Err(HostKeyError::Io),
        };

        let mut lines: Vec<String> = existing
            .lines()
            .filter(|line| line_key(line) != Some(key.as_str()))
            .map(str::to_owned)
            .collect();
        lines.push(format!("{} {}", key, fingerprint.as_str()));
        let mut content = lines.join("\n");
        content.push('\n');

        let parent = self
            .path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|_| HostKeyError::Io)?;
        let temp_path = self.temp_path(parent);
        tokio::fs::write(&temp_path, content)
            .await
            .map_err(|_| HostKeyError::Io)?;
        tokio::fs::rename(&temp_path, &self.path)
            .await
            .map_err(|_| HostKeyError::Io)?;
        Ok(())
    }
}
