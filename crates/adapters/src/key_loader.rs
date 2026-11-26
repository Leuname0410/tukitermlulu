//! Adaptador de carga de llaves privadas basado en `russh`.

use std::path::PathBuf;

use russh::keys::ssh_key::{self, LineEnding};
use tuki_core::auth::KeyRef;
use tuki_core::port_errors::KeyError;
use tuki_core::ports::{KeyLoader, PrivateKey, SecretString};

/// Carga llaves OpenSSH, PEM y PPK sin mantener estado.
#[derive(Default)]
pub struct RusshKeyLoader;

impl KeyLoader for RusshKeyLoader {
    async fn load(
        &self,
        key: &KeyRef,
        passphrase: Option<&SecretString>,
    ) -> Result<PrivateKey, KeyError> {
        let path = PathBuf::from(key.path());
        // La copia sigue envuelta para que la passphrase nunca quede en un `String` plano.
        let passphrase = passphrase.map(|secret| SecretString::new(secret.expose()));
        let result = tokio::task::spawn_blocking(move || {
            let password = passphrase.as_ref().map(SecretString::expose);
            let private_key = russh::keys::load_secret_key(path, password)?;
            let openssh = private_key.to_openssh(LineEnding::LF)?;
            Ok::<PrivateKey, russh::keys::Error>(PrivateKey::new(openssh.as_bytes()))
        })
        .await;

        match result {
            Ok(loaded) => loaded.map_err(map_error),
            // El hilo de carga terminó sin producir un resultado.
            Err(_) => Err(KeyError::Io),
        }
    }
}

/// Traduce errores de `russh` a `KeyError` sin propagar rutas ni secretos.
fn map_error(error: russh::keys::Error) -> KeyError {
    match error {
        russh::keys::Error::IO(io_error) if io_error.kind() == std::io::ErrorKind::NotFound => {
            KeyError::NotFound
        }
        russh::keys::Error::IO(_) => KeyError::Io,
        russh::keys::Error::KeyIsEncrypted => KeyError::PassphraseRequired,
        russh::keys::Error::SshKey(ssh_error) => match ssh_error {
            ssh_key::Error::Encrypted => KeyError::PassphraseRequired,
            ssh_key::Error::Crypto => KeyError::WrongPassphrase,
            // `PpkParseError` no se re-exporta: sus variantes se identifican por `Debug`.
            ssh_key::Error::Ppk(_) if is_ppk_variant(&ssh_error, "Encrypted") => {
                KeyError::PassphraseRequired
            }
            ssh_key::Error::Ppk(_) if is_ppk_variant(&ssh_error, "IncorrectMac") => {
                KeyError::WrongPassphrase
            }
            _ => KeyError::UnsupportedFormat,
        },
        _ => KeyError::UnsupportedFormat,
    }
}

/// Comprueba una variante interna del error PPK sin exponer su tipo.
fn is_ppk_variant(error: &ssh_key::Error, variant: &str) -> bool {
    format!("{error:?}").contains(variant)
}
