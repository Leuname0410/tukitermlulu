//! Registro de errores de la UI en el log del backend.

use crate::diagnostics::redact_line;

const MAX_SCOPE_CHARS: usize = 64;
const MAX_MESSAGE_CHARS: usize = 1024;

/// Normaliza el scope a caracteres seguros y lo recorta sin partir un carácter UTF-8.
fn sanitize_scope(scope: &str) -> String {
    scope
        .chars()
        .take(MAX_SCOPE_CHARS)
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '.' | '-') {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

/// Redacta campos sensibles y recorta el mensaje sin partir un carácter UTF-8.
fn sanitize_message(message: &str) -> String {
    let truncated: String = message.chars().take(MAX_MESSAGE_CHARS).collect();
    redact_line(&truncated)
}

/// Registra un error de la UI; el comando nunca falla y no expone secretos.
#[tauri::command]
pub(crate) fn log_ui_error(scope: String, message: String) {
    let scope = sanitize_scope(&scope);
    let message = sanitize_message(&message);
    tracing::warn!(event = "ui_error", scope, message);
}

#[cfg(test)]
mod tests {
    use super::{sanitize_message, sanitize_scope};

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
