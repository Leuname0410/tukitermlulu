//! Almacenes de secretos de sesión: keyring del SO y fake en memoria.

use std::collections::HashMap;
use std::sync::Mutex;

use keyring::{Entry, Error};
use tuki_core::port_errors::SecretError;
use tuki_core::ports::{SecretKey, SecretStore, SecretString};
use tuki_core::session::SessionId;

/// Almacén en memoria para pruebas; nunca persiste secretos en disco.
#[derive(Debug, Default)]
pub struct InMemorySecretStore {
    values: Mutex<HashMap<SessionId, SecretString>>,
}

impl SecretStore for InMemorySecretStore {
    async fn get(&self, key: &SecretKey) -> Result<Option<SecretString>, SecretError> {
        let values = self.values.lock().map_err(|_| SecretError::Io)?;
        Ok(values
            .get(&key.session)
            .map(|value| SecretString::new(value.expose())))
    }

    async fn set(&self, key: &SecretKey, value: &SecretString) -> Result<(), SecretError> {
        let mut values = self.values.lock().map_err(|_| SecretError::Io)?;
        values.insert(key.session, SecretString::new(value.expose()));
        Ok(())
    }

    async fn delete(&self, key: &SecretKey) -> Result<(), SecretError> {
        let mut values = self.values.lock().map_err(|_| SecretError::Io)?;
        values.remove(&key.session);
        Ok(())
    }
}

/// Almacén de contraseñas de sesión en el credential store del sistema (ADR-004).
#[derive(Debug)]
pub struct KeyringSecretStore {
    service: String,
}

impl KeyringSecretStore {
    /// Crea el adaptador para el servicio indicado, por ejemplo `tukitermlulu`.
    pub fn new(service: &str) -> Self {
        Self {
            service: service.to_owned(),
        }
    }

    /// Datos de la entrada: `user` deriva del identificador de sesión, nunca de passphrases.
    fn entry_data(&self, key: &SecretKey) -> (String, String) {
        (
            self.service.clone(),
            format!("session-{}", key.session.get()),
        )
    }
}

/// Traduce fallos del backend sin propagar detalles que puedan contener secretos.
fn map_keyring_error(error: Error) -> SecretError {
    match error {
        Error::NoStorageAccess(_) | Error::PlatformFailure(_) => SecretError::Unavailable,
        _ => SecretError::Io,
    }
}

impl SecretStore for KeyringSecretStore {
    async fn get(&self, key: &SecretKey) -> Result<Option<SecretString>, SecretError> {
        let (service, user) = self.entry_data(key);
        tokio::task::spawn_blocking(move || {
            let entry = Entry::new(&service, &user).map_err(map_keyring_error)?;
            match entry.get_password() {
                Ok(password) => Ok(Some(SecretString::new(password))),
                Err(Error::NoEntry) => Ok(None),
                Err(error) => Err(map_keyring_error(error)),
            }
        })
        .await
        .map_err(|_| SecretError::Io)?
    }

    async fn set(&self, key: &SecretKey, value: &SecretString) -> Result<(), SecretError> {
        let (service, user) = self.entry_data(key);
        let secret = SecretString::new(value.expose());
        tokio::task::spawn_blocking(move || {
            let entry = Entry::new(&service, &user).map_err(map_keyring_error)?;
            entry
                .set_password(secret.expose())
                .map_err(map_keyring_error)
        })
        .await
        .map_err(|_| SecretError::Io)?
    }

    async fn delete(&self, key: &SecretKey) -> Result<(), SecretError> {
        let (service, user) = self.entry_data(key);
        tokio::task::spawn_blocking(move || {
            let entry = Entry::new(&service, &user).map_err(map_keyring_error)?;
            match entry.delete_credential() {
                Ok(()) | Err(Error::NoEntry) => Ok(()),
                Err(error) => Err(map_keyring_error(error)),
            }
        })
        .await
        .map_err(|_| SecretError::Io)?
    }
}
