//! Integración: carga de llaves de `tests/fixtures` con el adaptador russh.

use std::path::{Path, PathBuf};

use tuki_adapters::key_loader::RusshKeyLoader;
use tuki_core::auth::KeyRef;
use tuki_core::port_errors::KeyError;
use tuki_core::ports::{KeyLoader, PrivateKey, SecretString};

const PASSPHRASE: &str = "tuki-pass";
const WRONG_PASSPHRASE: &str = "wrong-pass";

fn fixture(relative_path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(relative_path)
}

fn key_ref(relative_path: &str) -> KeyRef {
    let path = fixture(relative_path);
    let path = path.to_str().expect("ruta de fixture válida en UTF-8");
    KeyRef::new(path).expect("ruta de llave no vacía")
}

async fn load(relative_path: &str, password: Option<&str>) -> Result<PrivateKey, KeyError> {
    let secret = password.map(SecretString::from);
    RusshKeyLoader
        .load(&key_ref(relative_path), secret.as_ref())
        .await
}

fn assert_openssh_private_key(loaded: &PrivateKey) {
    let bytes = loaded.as_bytes();
    assert!(!bytes.is_empty(), "la llave cargada está vacía");
    let text = std::str::from_utf8(bytes).expect("salida OpenSSH en UTF-8");
    assert!(
        text.starts_with("-----BEGIN OPENSSH PRIVATE KEY-----"),
        "la llave no se re-serializó a OpenSSH"
    );
    assert!(
        !text.contains("ENCRYPTED"),
        "la llave re-serializada sigue cifrada"
    );
}

macro_rules! load_test {
    ($name:ident, $path:literal, $passphrase:expr) => {
        #[tokio::test]
        async fn $name() {
            let loaded = load($path, $passphrase)
                .await
                .unwrap_or_else(|error| panic!("no se cargó {}: {error:?}", $path));
            assert_openssh_private_key(&loaded);
        }
    };
}

macro_rules! key_error_test {
    ($name:ident, $path:literal, $passphrase:expr, $expected:expr) => {
        #[tokio::test]
        async fn $name() {
            assert_eq!(load($path, $passphrase).await.err(), Some($expected));
        }
    };
}

load_test!(should_load_ed25519_when_key_is_unencrypted, "ed25519", None);
load_test!(
    should_load_rsa_openssh_when_key_is_unencrypted,
    "keys/rsa_openssh",
    None
);
load_test!(
    should_load_rsa_openssh_pass_when_passphrase_is_correct,
    "keys/rsa_openssh_pass",
    Some(PASSPHRASE)
);
load_test!(
    should_load_rsa_pkcs1_when_key_is_pem,
    "keys/rsa_pkcs1.pem",
    None
);
load_test!(
    should_load_rsa_pkcs8_when_key_is_pem,
    "keys/rsa_pkcs8.pem",
    None
);
load_test!(
    should_load_ecdsa_p256_when_key_is_unencrypted,
    "keys/ecdsa_p256",
    None
);
load_test!(
    should_load_ecdsa_p256_pass_when_passphrase_is_correct,
    "keys/ecdsa_p256_pass",
    Some(PASSPHRASE)
);
load_test!(
    should_load_ed25519_pass_when_passphrase_is_correct,
    "keys/ed25519_pass",
    Some(PASSPHRASE)
);
load_test!(
    should_load_ppk_v2_when_key_is_unencrypted,
    "keys/ed25519_v2.ppk",
    None
);
load_test!(
    should_load_ppk_v2_pass_when_passphrase_is_correct,
    "keys/ed25519_v2_pass.ppk",
    Some(PASSPHRASE)
);
load_test!(
    should_load_ppk_v3_when_key_is_unencrypted,
    "keys/ed25519_v3.ppk",
    None
);
load_test!(
    should_load_ppk_v3_pass_when_passphrase_is_correct,
    "keys/ed25519_v3_pass.ppk",
    Some(PASSPHRASE)
);
load_test!(
    should_load_rsa_ppk_v3_when_key_is_unencrypted,
    "keys/rsa_v3.ppk",
    None
);

key_error_test!(
    should_require_passphrase_when_openssh_key_is_encrypted,
    "keys/ed25519_pass",
    None,
    KeyError::PassphraseRequired
);
key_error_test!(
    should_require_passphrase_when_ppk_v2_is_encrypted,
    "keys/ed25519_v2_pass.ppk",
    None,
    KeyError::PassphraseRequired
);
key_error_test!(
    should_require_passphrase_when_ppk_v3_is_encrypted,
    "keys/ed25519_v3_pass.ppk",
    None,
    KeyError::PassphraseRequired
);
key_error_test!(
    should_reject_wrong_passphrase_when_openssh_ed25519_is_encrypted,
    "keys/ed25519_pass",
    Some(WRONG_PASSPHRASE),
    KeyError::WrongPassphrase
);
key_error_test!(
    should_reject_wrong_passphrase_when_openssh_rsa_is_encrypted,
    "keys/rsa_openssh_pass",
    Some(WRONG_PASSPHRASE),
    KeyError::WrongPassphrase
);
key_error_test!(
    should_reject_wrong_passphrase_when_openssh_ecdsa_is_encrypted,
    "keys/ecdsa_p256_pass",
    Some(WRONG_PASSPHRASE),
    KeyError::WrongPassphrase
);
key_error_test!(
    should_reject_wrong_passphrase_when_ppk_v2_is_encrypted,
    "keys/ed25519_v2_pass.ppk",
    Some(WRONG_PASSPHRASE),
    KeyError::WrongPassphrase
);
key_error_test!(
    should_reject_wrong_passphrase_when_ppk_v3_is_encrypted,
    "keys/ed25519_v3_pass.ppk",
    Some(WRONG_PASSPHRASE),
    KeyError::WrongPassphrase
);

#[tokio::test]
async fn should_return_not_found_when_key_path_does_not_exist() {
    assert_eq!(
        load("keys/does-not-exist", None).await.err(),
        Some(KeyError::NotFound)
    );
}

#[tokio::test]
async fn should_return_unsupported_format_when_file_is_not_a_key() {
    assert_eq!(
        load("README.md", None).await.err(),
        Some(KeyError::UnsupportedFormat)
    );
}

#[tokio::test]
async fn should_ignore_passphrase_when_key_is_unencrypted() {
    let without_passphrase = load("ed25519", None)
        .await
        .expect("llave sin cifrar carga sin passphrase");
    let with_passphrase = load("ed25519", Some(PASSPHRASE))
        .await
        .expect("passphrase ignorada en llave sin cifrar");
    assert_eq!(without_passphrase.as_bytes(), with_passphrase.as_bytes());
}

#[tokio::test]
async fn should_not_leak_path_or_passphrase_when_formatting_error() {
    let secret = SecretString::from("super-secret-passphrase-do-not-leak");
    let error = RusshKeyLoader
        .load(&key_ref("keys/ed25519_pass"), Some(&secret))
        .await
        .expect_err("passphrase incorrecta");
    let debug = format!("{error:?}");
    let display = error.to_string();
    for message in [&debug, &display] {
        assert!(!message.contains("super-secret-passphrase-do-not-leak"));
        assert!(!message.contains("ed25519_pass"));
        assert!(!message.contains("fixtures"));
    }
}
