//! Configuración de logs estructurados y redacción de campos sensibles.

use std::fmt;
use std::fs;
use std::io;
use std::path::Path;

use tracing::field::{Field, Visit};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{Builder, Rotation};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::field::RecordFields;
use tracing_subscriber::fmt::format::{FormatFields, Writer};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

/// Nombres de campo cuyo valor nunca debe escribirse en el log.
const SENSITIVE_FIELD_NAMES: [&str; 5] =
    ["password", "passphrase", "secret", "private_key", "token"];

pub(crate) fn is_sensitive(name: &str) -> bool {
    let lowered = name.to_ascii_lowercase();
    SENSITIVE_FIELD_NAMES
        .iter()
        .any(|sensitive| lowered.contains(sensitive))
}

/// Formateador que sustituye por `[REDACTED]` el valor de los campos sensibles.
#[derive(Debug, Default)]
pub(crate) struct RedactingFields;

impl<'writer> FormatFields<'writer> for RedactingFields {
    fn format_fields<R: RecordFields>(&self, writer: Writer<'writer>, fields: R) -> fmt::Result {
        let mut visitor = RedactingVisitor {
            writer,
            first: true,
        };
        fields.record(&mut visitor);
        Ok(())
    }
}

struct RedactingVisitor<'writer> {
    writer: Writer<'writer>,
    first: bool,
}

impl RedactingVisitor<'_> {
    fn write_field(&mut self, name: &str, value: &dyn fmt::Display) {
        if !self.first {
            let _ = write!(self.writer, " ");
        }
        self.first = false;
        if is_sensitive(name) {
            let _ = write!(self.writer, "{name}=[REDACTED]");
        } else {
            let _ = write!(self.writer, "{name}={value}");
        }
    }
}

impl Visit for RedactingVisitor<'_> {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.write_field(field.name(), &format!("{value:?}"));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.write_field(field.name(), &value);
    }
}

/// Instala el subscriber global con archivo diario y devuelve su guardia.
pub(crate) fn init(log_dir: &Path) -> Result<WorkerGuard, io::Error> {
    fs::create_dir_all(log_dir)?;
    let appender = Builder::new()
        .rotation(Rotation::DAILY)
        .filename_prefix("tukitermlulu")
        .filename_suffix("log")
        .max_log_files(7)
        .build(log_dir)
        .map_err(io::Error::other)?;
    let (writer, guard) = tracing_appender::non_blocking(appender);
    let filter = EnvFilter::try_from_env("TUKI_LOG").unwrap_or_else(|_| EnvFilter::new("info"));
    let file_layer = tracing_subscriber::fmt::layer()
        .fmt_fields(RedactingFields)
        .with_ansi(false)
        .with_writer(writer);
    let subscriber = tracing_subscriber::registry().with(filter).with(file_layer);
    #[cfg(debug_assertions)]
    let subscriber = subscriber.with(
        tracing_subscriber::fmt::layer()
            .fmt_fields(RedactingFields)
            .with_ansi(false)
            .with_writer(io::stderr),
    );
    subscriber.try_init().map_err(io::Error::other)?;
    Ok(guard)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::{self, Write};
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};

    use tracing::Level;
    use tracing_subscriber::fmt::MakeWriter;
    use tracing_subscriber::prelude::*;
    use tuki_core::ports::SecretString;

    use super::{RedactingFields, init};

    /// Escritor en memoria para comprobar el texto emitido por la capa de formato.
    #[derive(Clone, Default)]
    struct MemoryWriter(Arc<Mutex<Vec<u8>>>);

    struct MemoryGuard(Arc<Mutex<Vec<u8>>>);

    impl Write for MemoryGuard {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl<'a> MakeWriter<'a> for MemoryWriter {
        type Writer = MemoryGuard;

        fn make_writer(&'a self) -> Self::Writer {
            MemoryGuard(Arc::clone(&self.0))
        }
    }

    fn captured_line(emit: impl FnOnce()) -> String {
        let writer = MemoryWriter::default();
        let buffer = Arc::clone(&writer.0);
        let subscriber = tracing_subscriber::registry().with(
            tracing_subscriber::fmt::layer()
                .fmt_fields(RedactingFields)
                .with_writer(writer)
                .with_ansi(false)
                .without_time()
                .with_target(false),
        );
        tracing::subscriber::with_default(subscriber, emit);
        String::from_utf8(buffer.lock().unwrap().clone()).unwrap()
    }

    #[test]
    fn should_redact_sensitive_fields_when_formatting_event() {
        let line = captured_line(|| {
            tracing::event!(
                Level::INFO,
                password = "s3cr3t",
                key_passphrase = "p4ss",
                api_token = "t0k",
                "evento de prueba"
            );
        });

        assert_eq!(line.matches("[REDACTED]").count(), 3, "{line}");
        assert!(!line.contains("s3cr3t"), "{line}");
        assert!(!line.contains("p4ss"), "{line}");
        assert!(!line.contains("t0k"), "{line}");
    }

    #[test]
    fn should_keep_non_sensitive_field_when_formatting_event() {
        let line = captured_line(|| {
            tracing::event!(Level::INFO, host = "srv", "evento de prueba");
        });

        assert!(line.contains("host=srv"), "{line}");
        assert!(!line.contains("[REDACTED]"), "{line}");
    }

    #[test]
    fn should_redact_secret_string_when_logged_with_debug() {
        let secret = SecretString::new("s3cr3t");
        let line = captured_line(|| {
            tracing::event!(Level::INFO, secret = ?secret, "evento de prueba");
        });

        assert!(line.contains("secret=[REDACTED]"), "{line}");
        assert!(!line.contains("s3cr3t"), "{line}");
    }

    fn unique_path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("tuki-log-test-{}-{label}", std::process::id()))
    }

    #[test]
    fn should_create_directory_when_init_receives_missing_dir() {
        let dir = unique_path("missing");
        let _ = fs::remove_dir_all(&dir);

        let guard = init(&dir).expect("init debería crear el directorio de logs");
        assert!(dir.is_dir());
        drop(guard);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn should_return_error_without_panic_when_init_receives_non_directory_path() {
        let path = unique_path("occupied");
        let _ = fs::remove_dir_all(&path);
        fs::write(&path, b"occupied").unwrap();

        let result = init(&path);
        assert!(result.is_err());
        let _ = fs::remove_file(&path);
    }

    /// Heurística: detecta palabras sensibles dentro de llamadas a macros de log.
    /// No sustituye a la revisión humana, pero impide reintroducirlas por accidente.
    const LOG_MACROS: [&str; 5] = ["info!", "debug!", "warn!", "error!", "trace!"];
    const FORBIDDEN_IN_LOG_CALLS: [&str; 3] = ["password", "passphrase", "bytes)"];

    fn collect_rust_files(dir: &Path, files: &mut Vec<PathBuf>) {
        assert!(dir.is_dir(), "falta el directorio {}", dir.display());
        for entry in fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_rust_files(&path, files);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                files.push(path);
            }
        }
    }

    /// Detecta el inicio de una llamada real: `info!(` y no la mención `info!` en texto.
    fn macro_offset(line: &str) -> Option<usize> {
        LOG_MACROS
            .iter()
            .filter_map(|name| {
                let mut start = 0;
                while let Some(position) = line[start..].find(name) {
                    let offset = start + position;
                    if line[offset + name.len()..].trim_start().starts_with('(') {
                        return Some(offset);
                    }
                    start = offset + name.len();
                }
                None
            })
            .min()
    }

    fn violations_in_file(path: &Path) -> Vec<String> {
        let content = fs::read_to_string(path).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        let mut violations = Vec::new();
        let mut index = 0;
        while index < lines.len() {
            let Some(offset) = macro_offset(lines[index]) else {
                index += 1;
                continue;
            };
            let mut call = String::new();
            let mut first = true;
            loop {
                let line = lines[index];
                call.push_str(if first { &line[offset..] } else { line });
                call.push(' ');
                let ends = if first {
                    line[offset..].contains(')')
                } else {
                    line.contains(')')
                };
                first = false;
                if ends || index + 1 == lines.len() {
                    break;
                }
                index += 1;
            }
            let lowered = call.to_ascii_lowercase();
            if FORBIDDEN_IN_LOG_CALLS
                .iter()
                .any(|word| lowered.contains(word))
            {
                violations.push(format!("{}:{}", path.display(), index + 1));
            }
            index += 1;
        }
        violations
    }

    #[test]
    fn should_reject_sensitive_log_fields_when_scanning_sources() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut files = Vec::new();
        for source in ["crates/adapters/src", "crates/desktop/src"] {
            collect_rust_files(&root.join(source), &mut files);
        }
        let violations: Vec<String> = files
            .iter()
            .flat_map(|path| violations_in_file(path))
            .collect();
        assert!(
            violations.is_empty(),
            "campos sensibles en logs: {violations:?}"
        );
    }
}
