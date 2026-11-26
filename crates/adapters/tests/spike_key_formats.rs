//! Spike T-06: comprueba qué formatos de llave carga `russh` de serie.

use std::path::{Path, PathBuf};

use russh::keys::{Error, PrivateKey};

const PASSPHRASE: &str = "tuki-pass";
const WRONG_PASSPHRASE: &str = "wrong-pass";

fn fixture(relative_path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(relative_path)
}

fn load_with_russh(relative_path: &str, password: Option<&str>) -> Result<PrivateKey, Error> {
    russh::keys::load_secret_key(fixture(relative_path), password)
}

fn assert_key_loads(relative_path: &str, password: Option<&str>) {
    let result = load_with_russh(relative_path, password);
    assert!(
        result.is_ok(),
        "russh no cargó {relative_path}: {:?}",
        result.err()
    );
}

fn assert_wrong_passphrase_fails(relative_path: &str) {
    let error = load_with_russh(relative_path, Some(WRONG_PASSPHRASE))
        .expect_err("russh aceptó una passphrase incorrecta");
    println!("{relative_path}: debug={error:?} | message={error}");
    assert!(!error.to_string().is_empty());
}

macro_rules! load_test {
    ($name:ident, $path:literal, $passphrase:expr) => {
        #[test]
        fn $name() {
            assert_key_loads($path, $passphrase);
        }
    };
}

macro_rules! wrong_passphrase_test {
    ($name:ident, $path:literal) => {
        #[test]
        fn $name() {
            assert_wrong_passphrase_fails($path);
        }
    };
}

load_test!(should_load_ed25519_when_using_russh, "ed25519", None);
load_test!(
    should_load_rsa_openssh_when_using_russh,
    "keys/rsa_openssh",
    None
);
load_test!(
    should_load_rsa_openssh_pass_when_using_russh,
    "keys/rsa_openssh_pass",
    Some(PASSPHRASE)
);
load_test!(
    should_load_rsa_pkcs1_pem_when_using_russh,
    "keys/rsa_pkcs1.pem",
    None
);
load_test!(
    should_load_rsa_pkcs8_pem_when_using_russh,
    "keys/rsa_pkcs8.pem",
    None
);
load_test!(
    should_load_ecdsa_p256_when_using_russh,
    "keys/ecdsa_p256",
    None
);
load_test!(
    should_load_ecdsa_p256_pass_when_using_russh,
    "keys/ecdsa_p256_pass",
    Some(PASSPHRASE)
);
load_test!(
    should_load_ed25519_pass_when_using_russh,
    "keys/ed25519_pass",
    Some(PASSPHRASE)
);
load_test!(
    should_load_ed25519_v2_ppk_when_using_russh,
    "keys/ed25519_v2.ppk",
    None
);
load_test!(
    should_load_ed25519_v2_pass_ppk_when_using_russh,
    "keys/ed25519_v2_pass.ppk",
    Some(PASSPHRASE)
);
load_test!(
    should_load_ed25519_v3_ppk_when_using_russh,
    "keys/ed25519_v3.ppk",
    None
);
load_test!(
    should_load_ed25519_v3_pass_ppk_when_using_russh,
    "keys/ed25519_v3_pass.ppk",
    Some(PASSPHRASE)
);
load_test!(
    should_load_rsa_v3_ppk_when_using_russh,
    "keys/rsa_v3.ppk",
    None
);

wrong_passphrase_test!(
    should_fail_ed25519_pass_when_passphrase_is_wrong,
    "keys/ed25519_pass"
);
wrong_passphrase_test!(
    should_fail_rsa_openssh_pass_when_passphrase_is_wrong,
    "keys/rsa_openssh_pass"
);
wrong_passphrase_test!(
    should_fail_ecdsa_p256_pass_when_passphrase_is_wrong,
    "keys/ecdsa_p256_pass"
);
wrong_passphrase_test!(
    should_fail_ed25519_v2_pass_ppk_when_passphrase_is_wrong,
    "keys/ed25519_v2_pass.ppk"
);
wrong_passphrase_test!(
    should_fail_ed25519_v3_pass_ppk_when_passphrase_is_wrong,
    "keys/ed25519_v3_pass.ppk"
);
