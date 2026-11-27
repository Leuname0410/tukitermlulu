#[cfg(test)]
mod tests {
    use std::fs::{self, File, FileTimes};
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, UNIX_EPOCH};

    use super::{build_report, timestamp, write_unique};

    fn temp_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("tuki-diagnostics-{}-{label}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_log(dir: &Path, name: &str, lines: &[String], modified: Duration) {
        let path = dir.join(name);
        let mut writer = File::create(&path).unwrap();
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

        assert!(report.contains("linea-499"), "{report}");
        assert!(report.contains("linea-2499"), "{report}");
        assert!(!report.contains("linea-498"), "{report}");
        assert_eq!(
            report.lines().filter(|line| line.starts_with("linea-")).count(),
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
