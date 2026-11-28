//! Exportación de un informe de diagnóstico con redacción de campos sensibles.

use std::collections::VecDeque;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Manager};

use crate::logging::is_sensitive;
use crate::sftp::downloads_dir;
use crate::terminal::CommandError;

type Res<T> = Result<T, CommandError>;

const LOG_PREFIX: &str = "tukitermlulu";
const MAX_LOG_LINES: usize = 2000;

/// Redacta los pares `campo=valor` sensibles conservando el resto del texto.
pub(crate) fn redact_line(line: &str) -> String {
    let mut redacted = String::with_capacity(line.len());
    for part in line.split_inclusive(char::is_whitespace) {
        let token = part.trim_end_matches(char::is_whitespace);
        let separator = &part[token.len()..];
        match token.split_once('=') {
            Some((name, _)) if is_sensitive(name) => {
                redacted.push_str(name);
                redacted.push_str("=[REDACTED]");
            }
            _ => redacted.push_str(token),
        }
        redacted.push_str(separator);
    }
    redacted
}

/// Defensa en profundidad: vuelve a redactar el informe antes de escribirlo.
fn redact_report(report: &str) -> String {
    let mut redacted = String::with_capacity(report.len());
    for line in report.split_inclusive('\n') {
        redacted.push_str(&redact_line(line));
    }
    redacted
}

/// Devuelve el `tukitermlulu*.log` con fecha de modificación más reciente.
fn latest_log(log_dir: &Path) -> io::Result<Option<PathBuf>> {
    let entries = match fs::read_dir(log_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let mut latest: Option<(SystemTime, PathBuf)> = None;
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !entry.file_type()?.is_file() || !name.starts_with(LOG_PREFIX) || !name.ends_with(".log")
        {
            continue;
        }
        let modified = entry.metadata()?.modified()?;
        let replace = latest
            .as_ref()
            .is_none_or(|(current, _)| modified > *current);
        if replace {
            latest = Some((modified, entry.path()));
        }
    }
    Ok(latest.map(|(_, path)| path))
}

/// Conserva solo las últimas `max_lines` líneas sin cargar el archivo completo.
fn read_last_lines(path: &Path, max_lines: usize) -> io::Result<VecDeque<String>> {
    let mut lines = VecDeque::new();
    if max_lines == 0 {
        return Ok(lines);
    }
    for line in BufReader::new(File::open(path)?).lines() {
        if lines.len() == max_lines {
            lines.pop_front();
        }
        lines.push_back(line?);
    }
    Ok(lines)
}

/// Compone el informe a partir del entorno y del último log disponible.
pub(crate) fn build_report(
    version: &str,
    os: &str,
    arch: &str,
    log_dir: &Path,
    max_lines: usize,
) -> io::Result<String> {
    let mut report = format!(
        "Informe de diagnóstico de {LOG_PREFIX}\nversion={version}\nos={os}\narch={arch}\n\nlogs:\n"
    );
    match latest_log(log_dir)? {
        Some(path) => {
            for line in read_last_lines(&path, max_lines)? {
                report.push_str(&line);
                report.push('\n');
            }
        }
        None => report.push_str("(sin logs)\n"),
    }
    Ok(redact_report(&report))
}

/// Convierte días desde el epoch Unix en fecha civil (UTC).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let shifted = days + 719_468;
    let era = if shifted >= 0 {
        shifted
    } else {
        shifted - 146_096
    } / 146_097;
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    };
    (year + i64::from(month <= 2), month as u32, day as u32)
}

/// Genera la marca temporal `AAAAMMDD-HHMMSS` en UTC.
fn timestamp(now: SystemTime) -> String {
    let seconds = now
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs());
    let (year, month, day) = civil_from_days((seconds / 86_400) as i64);
    let clock = seconds % 86_400;
    format!(
        "{year:04}{month:02}{day:02}-{:02}{:02}{:02}",
        clock / 3600,
        (clock / 60) % 60,
        clock % 60
    )
}

/// Escribe con `create_new`, añadiendo `-1`, `-2`, ... si el nombre ya existe.
fn write_unique(dir: &Path, stamp: &str, report: &str) -> io::Result<PathBuf> {
    for suffix in 0u32.. {
        let name = if suffix == 0 {
            format!("{LOG_PREFIX}-diagnostico-{stamp}.txt")
        } else {
            format!("{LOG_PREFIX}-diagnostico-{stamp}-{suffix}.txt")
        };
        let path = dir.join(name);
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                file.write_all(report.as_bytes())?;
                return Ok(path);
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::other(
        "no se pudo asignar un nombre al diagnóstico",
    ))
}

/// Genera el informe en Descargas y devuelve la ruta del archivo creado.
#[tauri::command]
pub(crate) async fn export_diagnostics(app: AppHandle) -> Res<String> {
    tauri::async_runtime::spawn_blocking(move || {
        let log_dir = app
            .path()
            .app_log_dir()
            .map_err(|_| CommandError::new("io", "No se pudo resolver la carpeta de logs"))?;
        let report = build_report(
            env!("CARGO_PKG_VERSION"),
            std::env::consts::OS,
            std::env::consts::ARCH,
            &log_dir,
            MAX_LOG_LINES,
        )
        .map_err(|_| CommandError::new("io", "No se pudo leer el log de la aplicación"))?;
        let downloads = downloads_dir(app.clone())
            .map_err(|_| CommandError::new("io", "No se pudo resolver la carpeta de Descargas"))?;
        let written = write_unique(
            Path::new(&downloads),
            &timestamp(SystemTime::now()),
            &report,
        )
        .map_err(|_| CommandError::new("io", "No se pudo escribir el diagnóstico en Descargas"))?;
        Ok(written.to_string_lossy().into_owned())
    })
    .await
    .map_err(|_| CommandError::new("io", "No se pudo completar la exportación del diagnóstico"))?
}

#[cfg(test)]
mod tests {
    use std::fs::{self, File, FileTimes};
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, UNIX_EPOCH};

    use super::{Res, build_report, export_diagnostics, timestamp, write_unique};
    use tauri::AppHandle;

    fn temp_dir(label: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("tuki-diagnostics-{}-{label}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_log(dir: &Path, name: &str, lines: &[String], modified: Duration) {
        let mut writer = File::create(dir.join(name)).unwrap();
        for line in lines {
            writeln!(writer, "{line}").unwrap();
        }
        writer
            .set_times(FileTimes::new().set_modified(UNIX_EPOCH + modified))
            .unwrap();
    }

    #[test]
    fn should_include_environment_and_redact_sensitive_fields_when_building_report() {
        let dir = temp_dir("fields");
        write_log(
            &dir,
            "tukitermlulu.old.log",
            &["solo-en-el-antiguo".to_owned()],
            Duration::from_secs(100),
        );
        write_log(
            &dir,
            "tukitermlulu.new.log",
            &[
                "password=abc host=srv".to_owned(),
                "token=xyz mensaje".to_owned(),
            ],
            Duration::from_secs(200),
        );

        let report = build_report("9.8.7", "testos", "testarch", &dir, 2000).unwrap();

        assert!(report.contains("version=9.8.7"), "{report}");
        assert!(report.contains("os=testos"), "{report}");
        assert!(report.contains("arch=testarch"), "{report}");
        assert!(report.contains("password=[REDACTED] host=srv"), "{report}");
        assert!(report.contains("token=[REDACTED] mensaje"), "{report}");
        assert!(!report.contains("password=abc"), "{report}");
        assert!(!report.contains("solo-en-el-antiguo"), "{report}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn should_keep_only_last_lines_when_building_report() {
        let dir = temp_dir("lines");
        let lines: Vec<String> = (0..2500).map(|index| format!("linea-{index}")).collect();
        write_log(&dir, "tukitermlulu.log", &lines, Duration::from_secs(300));

        let report = build_report("1", "os", "arch", &dir, 2000).unwrap();

        assert!(report.contains("linea-500"), "{report}");
        assert!(report.contains("linea-2499"), "{report}");
        assert!(!report.contains("linea-499"), "{report}");
        assert_eq!(
            report
                .lines()
                .filter(|line| line.starts_with("linea-"))
                .count(),
            2000
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn should_report_without_logs_when_directory_is_empty() {
        let dir = temp_dir("empty");

        let report = build_report("1", "os", "arch", &dir, 2000).unwrap();

        assert!(report.contains("(sin logs)"), "{report}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn should_report_without_logs_when_log_directory_is_missing() {
        let dir = temp_dir("missing");
        let missing = dir.join("inexistente");

        let report = build_report("1", "os", "arch", &missing, 2000).unwrap();

        assert!(report.contains("(sin logs)"), "{report}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn should_return_a_future_when_export_diagnostics_is_invoked() {
        fn assert_async_command<F, Fut>(_command: F)
        where
            F: FnOnce(AppHandle) -> Fut,
            Fut: std::future::Future<Output = Res<String>> + Send,
        {
        }

        assert_async_command(export_diagnostics);
    }

    #[test]
    fn should_create_distinct_files_without_overwriting_when_name_repeats() {
        let dir = temp_dir("unique");

        let first = write_unique(&dir, "20260101-010101", "primero").unwrap();
        let second = write_unique(&dir, "20260101-010101", "segundo").unwrap();

        assert_eq!(
            first.file_name().and_then(|name| name.to_str()),
            Some("tukitermlulu-diagnostico-20260101-010101.txt")
        );
        assert_eq!(
            second.file_name().and_then(|name| name.to_str()),
            Some("tukitermlulu-diagnostico-20260101-010101-1.txt")
        );
        assert_eq!(fs::read_to_string(&first).unwrap(), "primero");
        assert_eq!(fs::read_to_string(&second).unwrap(), "segundo");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn should_format_stamp_when_converting_time() {
        assert_eq!(timestamp(UNIX_EPOCH), "19700101-000000");
        assert_eq!(
            timestamp(UNIX_EPOCH + Duration::from_secs(1_700_000_000)),
            "20231114-221320"
        );
    }
}
