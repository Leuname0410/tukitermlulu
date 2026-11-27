//! Pruebas de integración de los almacenes de secretos de sesión.

use std::time::{SystemTime, UNIX_EPOCH};

use tuki_adapters::secret_store::{InMemorySecretStore, KeyringSecretStore};
use tuki_core::ports::{SecretKey, SecretStore, SecretString};
use tuki_core::session::SessionId;

fn key(id: u64) -> SecretKey {
    SecretKey {
        session: SessionId::new(id),
    }
}

async fn stored_value(store: &impl SecretStore, key: &SecretKey) -> Option<String> {
    store
        .get(key)
        .await
        .expect("lectura del almacén")
        .map(|value| value.expose().to_owned())
}

#[tokio::test(flavor = "current_thread")]
async fn should_return_value_when_setting_then_getting_in_memory_secret() {
    let store = InMemorySecretStore::default();
    let secret_key = key(1);

    assert_eq!(stored_value(&store, &secret_key).await, None);
    assert_eq!(
        store
            .set(&secret_key, &SecretString::new("first-password"))
            .await,
        Ok(())
    );
    assert_eq!(
        stored_value(&store, &secret_key).await,
        Some("first-password".to_owned())
    );
}

#[tokio::test(flavor = "current_thread")]
async fn should_replace_value_when_setting_twice_in_memory_secret() {
    let store = InMemorySecretStore::default();
    let secret_key = key(2);

    store
        .set(&secret_key, &SecretString::new("old-password"))
        .await
        .expect("primer almacenamiento");
    store
        .set(&secret_key, &SecretString::new("new-password"))
        .await
        .expect("reemplazo");

    assert_eq!(
        stored_value(&store, &secret_key).await,
        Some("new-password".to_owned())
    );
}

#[tokio::test(flavor = "current_thread")]
async fn should_return_none_when_deleting_in_memory_secret() {
    let store = InMemorySecretStore::default();
    let secret_key = key(3);

    store
        .set(&secret_key, &SecretString::new("to-delete"))
        .await
        .expect("almacenamiento");
    assert_eq!(store.delete(&secret_key).await, Ok(()));
    assert_eq!(stored_value(&store, &secret_key).await, None);
}

#[tokio::test(flavor = "current_thread")]
async fn should_succeed_when_deleting_missing_in_memory_secret_twice() {
    let store = InMemorySecretStore::default();
    let secret_key = key(4);

    assert_eq!(store.delete(&secret_key).await, Ok(()));
    assert_eq!(store.delete(&secret_key).await, Ok(()));
}

#[tokio::test(flavor = "current_thread")]
async fn should_not_contain_secrets_when_debug_formatting_in_memory_store() {
    let store = InMemorySecretStore::default();
    store
        .set(&key(5), &SecretString::new("very-secret-value"))
        .await
        .expect("almacenamiento");

    let debug = format!("{:?}", store);
    assert!(!debug.contains("very-secret-value"));
    assert!(debug.contains("[REDACTED]"));
}

#[tokio::test(flavor = "current_thread")]
#[ignore = "requires OS keyring"]
async fn should_round_trip_when_using_keyring_store() {
    let service = format!(
        "tukitermlulu-test-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("reloj del sistema")
            .as_nanos()
    );
    let store = KeyringSecretStore::new(&service);
    let secret_key = key(42);

    assert_eq!(stored_value(&store, &secret_key).await, None);
    assert_eq!(
        store
            .set(&secret_key, &SecretString::new("first-password"))
            .await,
        Ok(())
    );
    assert_eq!(
        stored_value(&store, &secret_key).await,
        Some("first-password".to_owned())
    );
    assert_eq!(
        store
            .set(&secret_key, &SecretString::new("second-password"))
            .await,
        Ok(())
    );
    assert_eq!(
        stored_value(&store, &secret_key).await,
        Some("second-password".to_owned())
    );
    assert_eq!(store.delete(&secret_key).await, Ok(()));
    assert_eq!(stored_value(&store, &secret_key).await, None);
    assert_eq!(
        store.delete(&secret_key).await,
        Ok(()),
        "borrar dos veces debe ser idempotente"
    );
}
