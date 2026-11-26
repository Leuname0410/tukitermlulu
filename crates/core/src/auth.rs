//! Entidades de autenticación: referencias a llaves y método de autenticación.

#[cfg(test)]
mod tests {
    use super::{AuthMethod, KeyRef};
    use crate::error::DomainError;

    #[test]
    fn should_reject_blank_when_creating_key_ref() {
        assert_eq!(KeyRef::new(""), Err(DomainError::EmptyKeyPath));
        assert_eq!(KeyRef::new("   "), Err(DomainError::EmptyKeyPath));
    }

    #[test]
    fn should_trim_whitespace_when_creating_key_ref() {
        assert_eq!(
            KeyRef::new("  /home/alice/.ssh/id_ed25519  ").map(|key| key.path().to_owned()),
            Ok("/home/alice/.ssh/id_ed25519".to_owned())
        );
    }

    #[test]
    fn should_allow_not_remembering_password_when_building_password_method() {
        let auth = AuthMethod::Password { remember: false };
        assert!(matches!(auth, AuthMethod::Password { remember: false }));
    }

    #[test]
    fn should_expose_only_non_secret_flags_when_matching_auth_methods() {
        // El match sin `..` falla en compilación si aparece un campo secreto nuevo.
        for method in [
            AuthMethod::Password { remember: true },
            AuthMethod::PublicKey {
                key: KeyRef::new("/home/alice/.ssh/id_ed25519").expect("ruta válida"),
                has_passphrase: true,
            },
        ] {
            match method {
                AuthMethod::Password { remember } => assert!(remember),
                AuthMethod::PublicKey {
                    key,
                    has_passphrase,
                } => {
                    assert_eq!(key.path(), "/home/alice/.ssh/id_ed25519");
                    assert!(has_passphrase);
                }
            }
        }
    }
}
