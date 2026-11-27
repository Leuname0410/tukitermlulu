//! Caso de uso para validar e importar llaves privadas.

use tuki_core::auth::KeyRef;
use tuki_core::port_errors::KeyError;
use tuki_core::ports::{KeyFileStore, KeyLoader, SecretString};

/// Entrada del caso de uso de importación.
pub struct ImportKeyRequest<'a> {
    pub source: KeyRef,
    pub copy_to_app_dir: bool,
    pub passphrase: Option<&'a SecretString>,
}

/// Resultado de la importación.
#[derive(Debug)]
pub struct ImportedKey {
    pub key: KeyRef,
    pub has_passphrase: bool,
}

/// Valida una llave y, si se pide, la copia al directorio privado de la app.
pub struct ImportKey<L: KeyLoader, S: KeyFileStore> {
    loader: L,
    store: S,
}

impl<L: KeyLoader, S: KeyFileStore> ImportKey<L, S> {
    /// Crea el caso de uso con los puertos que necesita.
    pub fn new(loader: L, store: S) -> Self {
        Self { loader, store }
    }

    /// Importa la llave indicada por la petición.
    pub async fn execute(&self, request: ImportKeyRequest<'_>) -> Result<ImportedKey, KeyError> {
        let has_passphrase = match self.loader.load(&request.source, request.passphrase).await {
            Ok(loaded_key) => {
                drop(loaded_key);
                request.passphrase.is_some()
            }
            Err(KeyError::PassphraseRequired) if request.passphrase.is_none() => true,
            Err(error) => return Err(error),
        };

        let key = if request.copy_to_app_dir {
            self.store.import(&request.source).await?
        } else {
            request.source
        };

        Ok(ImportedKey {
            key,
            has_passphrase,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::task::{Context, Poll, Waker};

    use tuki_core::auth::KeyRef;
    use tuki_core::port_errors::KeyError;
    use tuki_core::ports::{MockKeyFileStore, MockKeyLoader, PrivateKey, SecretString};

    use super::{ImportKey, ImportKeyRequest};

    fn block_on<F: Future>(future: F) -> F::Output {
        let mut context = Context::from_waker(Waker::noop());
        let mut future = std::pin::pin!(future);
        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => output,
            Poll::Pending => panic!("future pending"),
        }
    }

    fn source() -> KeyRef {
        KeyRef::new("~/.ssh/id_ed25519").unwrap()
    }

    fn stored() -> KeyRef {
        KeyRef::new("/home/alice/.generador/keys/id_ed25519").unwrap()
    }

    fn ok_loader() -> MockKeyLoader {
        let mut loader = MockKeyLoader::new();
        loader
            .expect_load()
            .once()
            .returning(|_, _| Box::pin(async { Ok(PrivateKey::new(vec![1, 2, 3])) }));
        loader
    }

    fn no_import_store() -> MockKeyFileStore {
        let mut store = MockKeyFileStore::new();
        store
            .expect_import()
            .never()
            .returning(|_| Box::pin(async { Ok(stored()) }));
        store
    }

    #[test]
    fn should_return_source_when_key_is_valid_and_copy_is_not_requested() {
        let use_case = ImportKey::new(ok_loader(), no_import_store());

        let imported = block_on(use_case.execute(ImportKeyRequest {
            source: source(),
            copy_to_app_dir: false,
            passphrase: None,
        }))
        .unwrap();

        assert_eq!(imported.key, source());
        assert!(!imported.has_passphrase);
    }

    #[test]
    fn should_return_store_key_when_copy_is_requested() {
        let mut store = MockKeyFileStore::new();
        store
            .expect_import()
            .once()
            .withf(|key| *key == source())
            .returning(|_| Box::pin(async { Ok(stored()) }));
        let use_case = ImportKey::new(ok_loader(), store);

        let imported = block_on(use_case.execute(ImportKeyRequest {
            source: source(),
            copy_to_app_dir: true,
            passphrase: None,
        }))
        .unwrap();

        assert_eq!(imported.key, stored());
        assert!(!imported.has_passphrase);
    }

    #[test]
    fn should_report_passphrase_when_key_loads_with_passphrase() {
        let mut loader = MockKeyLoader::new();
        loader.expect_load().once().returning(|_, passphrase| {
            assert!(passphrase.is_some());
            Box::pin(async { Ok(PrivateKey::new(vec![1, 2, 3])) })
        });
        let use_case = ImportKey::new(loader, no_import_store());
        let passphrase = SecretString::new("secret");

        let imported = block_on(use_case.execute(ImportKeyRequest {
            source: source(),
            copy_to_app_dir: false,
            passphrase: Some(&passphrase),
        }))
        .unwrap();

        assert!(imported.has_passphrase);
    }

    #[test]
    fn should_report_encrypted_key_when_passphrase_is_required_but_absent() {
        let mut loader = MockKeyLoader::new();
        loader.expect_load().once().returning(|_, passphrase| {
            assert!(passphrase.is_none());
            Box::pin(async { Err(KeyError::PassphraseRequired) })
        });
        let use_case = ImportKey::new(loader, no_import_store());

        let imported = block_on(use_case.execute(ImportKeyRequest {
            source: source(),
            copy_to_app_dir: false,
            passphrase: None,
        }))
        .unwrap();

        assert_eq!(imported.key, source());
        assert!(imported.has_passphrase);
    }

    #[test]
    fn should_propagate_loader_error_when_key_is_not_usable() {
        for error in [
            KeyError::WrongPassphrase,
            KeyError::UnsupportedFormat,
            KeyError::NotFound,
        ] {
            let mut loader = MockKeyLoader::new();
            loader
                .expect_load()
                .once()
                .returning(move |_, _| Box::pin(async move { Err(error) }));
            let use_case = ImportKey::new(loader, no_import_store());

            let result = block_on(use_case.execute(ImportKeyRequest {
                source: source(),
                copy_to_app_dir: false,
                passphrase: None,
            }));

            assert_eq!(result.unwrap_err(), error);
        }
    }

    #[test]
    fn should_propagate_error_when_store_import_fails() {
        let mut store = MockKeyFileStore::new();
        store
            .expect_import()
            .once()
            .returning(|_| Box::pin(async { Err(KeyError::Io) }));
        let use_case = ImportKey::new(ok_loader(), store);

        let result = block_on(use_case.execute(ImportKeyRequest {
            source: source(),
            copy_to_app_dir: true,
            passphrase: None,
        }));

        assert_eq!(result.unwrap_err(), KeyError::Io);
    }
}
