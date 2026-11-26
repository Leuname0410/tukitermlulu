//! Value objects de host: nombre, puerto y usuario.

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
            assert_eq!(Hostname::new(value).map(|h| h.as_str()), Ok(value));
        }
    }

    #[test]
    fn should_trim_surrounding_whitespace_when_creating_hostname() {
        assert_eq!(
            Hostname::new("  example.com  ").map(|h| h.as_str()),
            Ok("example.com")
        );
    }

    #[test]
    fn should_reject_empty_when_creating_username() {
        assert_eq!(Username::new(""), Err(DomainError::EmptyUsername));
    }

    #[test]
    fn should_keep_name_when_creating_username() {
        assert_eq!(Username::new("alice").map(|u| u.as_str()), Ok("alice"));
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
