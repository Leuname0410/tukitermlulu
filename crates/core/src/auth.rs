//! Entidades de autenticación: referencias a llaves y método de autenticación.

use crate::error::DomainError;

/// Referencia local al archivo de llave. Nunca contiene la llave ni su passphrase.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyRef {
    path: String,
}

impl KeyRef {
    pub fn new(path: &str) -> Result<Self, DomainError> {
        let trimmed = path.trim();
        if trimmed.is_empty() {
            return Err(DomainError::EmptyKeyPath);
        }
        Ok(Self {
            path: trimmed.to_owned(),
        })
    }

    pub fn path(&self) -> &str {
        &self.path
    }
}

/// Método de autenticación de una sesión. Solo guarda banderas: los secretos van al `SecretStore`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthMethod {
    /// Contraseña: `remember` indica si se guarda en el `SecretStore`.
    Password { remember: bool },
    /// Llave pública: `has_passphrase` solo indica si la llave está cifrada.
    PublicKey { key: KeyRef, has_passphrase: bool },
}

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
