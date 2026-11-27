#![cfg(feature = "integration")]

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::sync::Notify;
use tuki_adapters::key_loader::RusshKeyLoader;
use tuki_adapters::sftp::RusshFileSystem;
use tuki_adapters::ssh_transport::RusshTransport;
use tuki_core::auth::KeyRef;
use tuki_core::host::{Host, Hostname, Port, Username};
use tuki_core::ports::{
    CancelToken, Connected, Credential, FileTransfer, KeyLoader, PtySink, PtySize, SshTransport,
    TransferProgress,
};
use tuki_core::remote_path::RemotePath;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kpi {
    ConnectMs,
    EchoP95Ms,
    SftpRatio,
}

impl Kpi {
    fn as_str(self) -> &'static str {
        match self {
            Self::ConnectMs => "connect_ms",
            Self::EchoP95Ms => "echo_p95_ms",
            Self::SftpRatio => "sftp_ratio",
        }
    }

    fn higher_is_better(self) -> bool {
        matches!(self, Self::SftpRatio)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Status {
    Pass,
    Fail,
    Skipped,
}

type Row = (Kpi, Option<f64>, f64, Option<f64>, Status);

fn median(mut values: Vec<f64>) -> f64 {
    values.sort_by(f64::total_cmp);
    match values.len() {
        0 => f64::NAN,
        len if len % 2 == 1 => values[len / 2],
        len => (values[len / 2 - 1] + values[len / 2]) / 2.0,
    }
}

fn p95(mut values: Vec<f64>) -> f64 {
    values.sort_by(f64::total_cmp);
    if values.is_empty() {
        return f64::NAN;
    }
    values[(values.len() * 95).div_ceil(100) - 1]
}

fn evaluate(kpi: Kpi, value: f64, target: f64, baseline: Option<f64>) -> Status {
    let worse = |base: f64| {
        if kpi.higher_is_better() {
            value < base * 0.8
        } else {
            value > base * 1.2
        }
    };
    let met = if kpi.higher_is_better() {
        value >= target
    } else {
        value <= target
    };
    if baseline.is_some_and(worse) || !met {
        Status::Fail
    } else {
        Status::Pass
    }
}

#[test]
fn should_return_median_when_five_samples_are_provided() {
    assert_eq!(median(vec![5.0, 1.0, 3.0, 4.0, 2.0]), 3.0);
}

#[test]
fn should_return_95th_percentile_when_200_keystrokes_are_measured() {
    let values: Vec<f64> = (1..=200).map(f64::from).collect();
    assert_eq!(p95(values), 190.0);
}

#[test]
fn should_fail_when_target_is_missed() {
    assert_eq!(evaluate(Kpi::ConnectMs, 1600.0, 1500.0, None), Status::Fail);
}

#[test]
fn should_fail_when_baseline_regresses_by_21_percent() {
    assert_eq!(
        evaluate(Kpi::EchoP95Ms, 121.0, 200.0, Some(100.0)),
        Status::Fail
    );
}

#[test]
fn should_pass_when_baseline_regresses_by_19_percent() {
    assert_eq!(
        evaluate(Kpi::EchoP95Ms, 119.0, 200.0, Some(100.0)),
        Status::Pass
    );
}

#[test]
fn should_fail_when_sftp_ratio_drops_below_baseline() {
    assert_eq!(evaluate(Kpi::SftpRatio, 0.79, 0.8, Some(1.0)), Status::Fail);
}

fn fixture(relative: &str) -> String {
    format!(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures/{}"),
        relative
    )
}

/// Estado del PTY: bytes recibidos y avisos para no sondear en bucle.
#[derive(Clone)]
struct Echo {
    bytes: Arc<Mutex<Vec<u8>>>,
    notify: Arc<Notify>,
}

impl Echo {
    fn new() -> (EchoSink, Self) {
        let echo = Self {
            bytes: Arc::new(Mutex::new(Vec::new())),
            notify: Arc::new(Notify::new()),
        };
        (EchoSink(echo.clone()), echo)
    }

    fn len(&self) -> usize {
        self.bytes.lock().expect("buffer del PTY").len()
    }
}

struct EchoSink(Echo);

impl PtySink for EchoSink {
    fn data(&mut self, bytes: &[u8]) {
        self.0
            .bytes
            .lock()
            .expect("buffer del PTY")
            .extend_from_slice(bytes);
        self.0.notify.notify_one();
    }

    fn closed(&mut self, _exit_status: Option<u32>) {
        self.0.notify.notify_one();
    }
}

async fn wait_for_echo(echo: &Echo, expected: usize) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let notified = echo.notify.notified();
            tokio::pin!(notified);
            if echo.len() >= expected {
                return;
            }
            notified.await;
        }
    })
    .await
    .expect("timeout esperando el eco del PTY");
}

/// Cinco repeticiones de 200 pulsaciones; cada repetición aporta su p95.
async fn measure_echo(transport: &RusshTransport, connected: &Connected) -> Vec<f64> {
    let (sink, echo) = Echo::new();
    let channel = transport
        .open_pty(
            connected.id,
            PtySize::new(24, 80).expect("tamaño del PTY"),
            Box::new(sink),
        )
        .await
        .expect("abrir el PTY");
    transport
        .write(channel, b"stty raw -echo\n")
        .await
        .expect("configurar el PTY");
    tokio::time::sleep(Duration::from_millis(100)).await;
    transport
        .write(channel, b"cat\n")
        .await
        .expect("iniciar cat");
    tokio::time::sleep(Duration::from_millis(100)).await;

    let mut samples = Vec::new();
    for _ in 0..5 {
        let mut keystrokes = Vec::with_capacity(200);
        for _ in 0..200 {
            let before = echo.len();
            let started = Instant::now();
            transport
                .write(channel, b"x")
                .await
                .expect("escribir una tecla");
            wait_for_echo(&echo, before + 1).await;
            keystrokes.push(started.elapsed().as_secs_f64() * 1000.0);
        }
        samples.push(p95(keystrokes));
    }
    samples
}

struct NoopProgress;

impl TransferProgress for NoopProgress {
    fn progress(&self, _transferred: u64, _total: Option<u64>) {}
}

/// Referencia OpenSSH; `None` si `sftp` no está en el PATH o falla.
fn openssh_download(destination: &Path, working_dir: &Path) -> Option<f64> {
    let key = fixture("ed25519");
    let null = if cfg!(windows) { "NUL" } else { "/dev/null" };
    let known_hosts = format!("UserKnownHostsFile={null}");
    let destination = destination.to_string_lossy().replace('\\', "/");
    let started = Instant::now();
    let mut child = Command::new("sftp")
        .args([
            "-b",
            "-",
            "-i",
            key.as_str(),
            "-P",
            "2222",
            "-o",
            "StrictHostKeyChecking=no",
            "-o",
            known_hosts.as_str(),
            "tuki@127.0.0.1",
        ])
        .current_dir(working_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    {
        let mut stdin = child.stdin.take().expect("stdin del sftp");
        let _ = writeln!(stdin, "get /tmp/kpi.bin {destination}");
        let _ = writeln!(stdin, "bye");
    }
    if !child.wait().ok()?.success() {
        eprintln!("sftp de referencia falló; ratio marcado como skipped");
        return None;
    }
    Some(started.elapsed().as_secs_f64() * 1000.0)
}

/// Ratio de velocidad: tiempo de OpenSSH / tiempo propio (mayor es mejor).
async fn measure_sftp(transport: &RusshTransport, connected: &Connected, dir: &Path) -> Vec<f64> {
    let fs = RusshFileSystem::new(transport.clone());
    let source = RemotePath::new("/tmp/kpi.bin").expect("ruta remota del benchmark");
    let mut ratios = Vec::new();
    for index in 0..5 {
        let ours = dir.join(format!("ours-{index}.bin"));
        let started = Instant::now();
        fs.download(
            connected.id,
            &source,
            &ours,
            &NoopProgress,
            &CancelToken::default(),
        )
        .await
        .expect("descargar con el cliente de tuki");
        let ours_ms = started.elapsed().as_secs_f64() * 1000.0;
        std::fs::remove_file(&ours).expect("borrar descarga local");

        let reference = dir.join(format!("openssh-{index}.bin"));
        let Some(reference_ms) = openssh_download(&reference, dir) else {
            continue;
        };
        std::fs::remove_file(&reference).expect("borrar descarga de referencia");
        ratios.push(reference_ms / ours_ms);
    }
    ratios
}

fn baseline_file() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/kpi-baseline.json")
}

fn load_baseline() -> serde_json::Value {
    let contents = std::fs::read_to_string(baseline_file()).expect("leer docs/kpi-baseline.json");
    serde_json::from_str(&contents).expect("JSON de baseline inválido")
}

fn number(config: &serde_json::Value, kpi: Kpi, section: &str) -> Option<f64> {
    config[section][kpi.as_str()].as_f64()
}

fn write_baseline(config: &serde_json::Value, rows: &[Row]) {
    let measured = |field: &str| -> String {
        rows.iter()
            .find(|row| row.0.as_str() == field)
            .and_then(|row| row.1)
            .map(|value| format!("{:.2}", (value * 100.0).round() / 100.0))
            .unwrap_or_else(|| "null".to_owned())
    };
    let rendered = format!(
        "{{\n  \"targets\": {{\n    \"connect_ms\": {},\n    \"echo_p95_ms\": {},\n    \"sftp_ratio\": {}\n  }},\n  \"baseline\": {{\n    \"connect_ms\": {},\n    \"echo_p95_ms\": {},\n    \"sftp_ratio\": {}\n  }}\n}}\n",
        config["targets"]["connect_ms"],
        config["targets"]["echo_p95_ms"],
        config["targets"]["sftp_ratio"],
        measured("connect_ms"),
        measured("echo_p95_ms"),
        measured("sftp_ratio"),
    );
    std::fs::write(baseline_file(), rendered).expect("escribir docs/kpi-baseline.json");
}

fn render_table(rows: &[Row]) -> String {
    let mut table = String::from(
        "| KPI | mediana | target | baseline | estado |\n| --- | ---: | ---: | ---: | --- |\n",
    );
    for (kpi, value, target, baseline, status) in rows {
        let format = |value: Option<f64>| match value {
            None => "—".to_owned(),
            Some(value) if kpi.higher_is_better() => format!("{value:.2}"),
            Some(value) => format!("{value:.1} ms"),
        };
        table.push_str(&format!(
            "| {} | {} | {} | {} | {} |\n",
            kpi.as_str(),
            format(*value),
            format(Some(*target)),
            format(*baseline),
            format!("{status:?}").to_uppercase(),
        ));
    }
    table
}

#[tokio::test]
#[ignore = "solo se ejecuta con just kpi"]
async fn kpi_benchmarks() {
    let config = load_baseline();
    let host = Host {
        hostname: Hostname::new("127.0.0.1").expect("hostname del fixture"),
        port: Port::new(2222).expect("puerto del fixture"),
        username: Username::new("tuki").expect("usuario del fixture"),
    };

    let mut connect = Vec::new();
    let mut session = None;
    for index in 0..5 {
        let transport = RusshTransport::new();
        let started = Instant::now();
        let connected = transport.connect(&host).await.expect("conectar al fixture");
        let path = fixture("ed25519");
        let key = RusshKeyLoader
            .load(&KeyRef::new(&path).expect("ruta de la llave"), None)
            .await
            .expect("cargar llave ed25519");
        transport
            .authenticate(connected.id, Credential::PublicKey(key))
            .await
            .expect("autenticar con llave ed25519");
        connect.push(started.elapsed().as_secs_f64() * 1000.0);
        if index == 4 {
            session = Some((transport, connected));
        } else {
            transport.close(connected.id).await.expect("cerrar sesión");
        }
    }
    let (transport, connected) = session.expect("cinco sesiones de benchmark");
    transport
        .exec(connected.id, "head -c 67108864 /dev/urandom > /tmp/kpi.bin")
        .await
        .expect("generar el archivo de 64 MiB");

    let echo = measure_echo(&transport, &connected).await;
    let directory = tempfile::tempdir().expect("directorio temporal");
    let sftp = measure_sftp(&transport, &connected, directory.path()).await;

    let mut rows = Vec::new();
    for (kpi, samples) in [
        (Kpi::ConnectMs, connect),
        (Kpi::EchoP95Ms, echo),
        (Kpi::SftpRatio, sftp),
    ] {
        let value = (!samples.is_empty()).then(|| median(samples));
        let target = number(&config, kpi, "targets").expect("target del KPI configurado");
        let baseline = number(&config, kpi, "baseline");
        let status = value.map_or(Status::Skipped, |sample| {
            evaluate(kpi, sample, target, baseline)
        });
        rows.push((kpi, value, target, baseline, status));
    }

    let table = render_table(&rows);
    println!("{table}");
    if let Ok(path) = std::env::var("GITHUB_STEP_SUMMARY") {
        let mut summary = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .expect("abrir GITHUB_STEP_SUMMARY");
        writeln!(summary, "{table}").expect("escribir GITHUB_STEP_SUMMARY");
    }
    if std::env::var_os("KPI_WRITE_BASELINE").is_some() {
        write_baseline(&config, &rows);
    }

    let failures: Vec<&str> = rows
        .iter()
        .filter(|row| row.4 == Status::Fail)
        .map(|row| row.0.as_str())
        .collect();
    assert!(failures.is_empty(), "KPI fuera de umbral: {failures:?}");
}
