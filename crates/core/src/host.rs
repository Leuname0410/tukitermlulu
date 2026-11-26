//! Value objects de host: nombre, puerto y usuario.

use crate::error::DomainError;

/// Puerto TCP válido (1..=65535). Por defecto, 22.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Port(u16);

impl Port {
    /// Crea un puerto validando que esté en el rango permitido.
    pub fn new(v: u16) -> Result<Self, DomainError> {
        if v == 0 {
            return Err(DomainError::InvalidPort);
        }
        Ok(Self(v))
    }

    /// Devuelve el valor numérico del puerto.
    pub fn get(self) -> u16 {
        self.0
    }
}

impl Default for Port {
    fn default() -> Self {
        Self(22)
    }
}

/// Nombre de host: DNS, IPv4 o IPv6 (con o sin corchetes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hostname(String);

impl Hostname {
    /// Crea un nombre de host recortando espacios y rechazando caracteres inválidos.
    pub fn new(v: &str) -> Result<Self, DomainError> {
        let trimmed = v.trim();
        if trimmed.is_empty() {
            return Err(DomainError::EmptyHost);
        }
        if trimmed.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err(DomainError::InvalidHost);
        }
        Ok(Self(trimmed.to_owned()))
    }

    /// Devuelve el nombre de host como cadena.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Nombre de usuario del servidor remoto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Username(String);

impl Username {
    /// Crea un usuario validando que no esté vacío.
    pub fn new(v: &str) -> Result<Self, DomainError> {
        let trimmed = v.trim();
        if trimmed.is_empty() {
            return Err(DomainError::EmptyUsername);
        }
        Ok(Self(trimmed.to_owned()))
    }

    /// Devuelve el usuario como cadena.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Destino de conexión: nombre de host, puerto y usuario.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Host {
    pub hostname: Hostname,
    pub port: Port,
    pub username: Username,
}

#[cfg(test)]
mod tests {
    use super::{Host, Hostname, Port, Username};
    use crate::error::DomainError;

    #[test]
    fn should_reject_zero_when_creating_port() {
        assert_eq!(Port::new(0), Err(DomainError::InvalidPort));
    }

    #[test]
    fn should_accept_boundaries_when_creating_port() {
        assert_eq!(Port::new(1).map(Port::get), Ok(1));
        assert_eq!(Port::new(u16::MAX).map(Port::get), Ok(65535));
    }

    #[test]
    fn should_default_to_22_when_using_port_default() {
        assert_eq!(Port::default().get(), 22);
    }

    #[test]
    fn should_reject_blank_when_creating_hostname() {
        assert_eq!(Hostname::new(""), Err(DomainError::EmptyHost));
        assert_eq!(Hostname::new("   "), Err(DomainError::EmptyHost));
    }

    #[test]
    fn should_reject_internal_whitespace_or_control_when_creating_hostname() {
        assert_eq!(Hostname::new("a b"), Err(DomainError::InvalidHost));
        assert_eq!(Hostname::new("a\tb"), Err(DomainError::InvalidHost));
    }

    #[test]
    fn should_accept_common_addresses_when_creating_hostname() {
        for value in ["example.com", "10.0.0.1", "::1", "[::1]"] {
            assert_eq!(
                Hostname::new(value).map(|h| h.as_str().to_owned()),
                Ok(value.to_owned())
            );
        }
    }

    #[test]
    fn should_trim_surrounding_whitespace_when_creating_hostname() {
        assert_eq!(
            Hostname::new("  example.com  ").map(|h| h.as_str().to_owned()),
            Ok("example.com".to_owned())
        );
    }

    #[test]
    fn should_reject_empty_when_creating_username() {
        assert_eq!(Username::new(""), Err(DomainError::EmptyUsername));
    }

    #[test]
    fn should_keep_name_when_creating_username() {
        assert_eq!(
            Username::new("alice").map(|u| u.as_str().to_owned()),
            Ok("alice".to_owned())
        );
    }

    #[test]
    fn should_expose_all_fields_when_building_host() {
        let host = Host {
            hostname: Hostname::new("example.com").expect("host válido"),
            port: Port::new(2222).expect("puerto válido"),
            username: Username::new("alice").expect("usuario válido"),
        };

        assert_eq!(host.hostname.as_str(), "example.com");
        assert_eq!(host.port.get(), 2222);
        assert_eq!(host.username.as_str(), "alice");
    }
}
