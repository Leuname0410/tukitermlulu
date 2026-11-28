//! Registro de errores de la UI en el log del backend.

#[cfg(test)]
mod tests {
    use super::{sanitize_scope, sanitize_message};

    #[test]
    fn should_sanitize_invalid_scope_characters_when_processing_scope() {
        assert_eq!(sanitize_scope("tabs/connect!"), "tabs_connect_");
    }

    #[test]
    fn should_truncate_long_scope_when_processing_scope() {
        let scope = "a".repeat(100);
        assert_eq!(sanitize_scope(&scope), "a".repeat(64));
    }

    #[test]
    fn should_redact_sensitive_fields_when_processing_message() {
        assert_eq!(sanitize_message("password=abc"), "password=[REDACTED]");
    }

    #[test]
    fn should_truncate_multibyte_message_when_processing_message() {
        let message = "🙂".repeat(5000);
        assert_eq!(sanitize_message(&message), "🙂".repeat(1024));
    }
}
