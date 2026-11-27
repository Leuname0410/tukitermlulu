#![cfg(feature = "integration")]

/// KPI medido por el benchmark; determina qué dirección es "mejor".
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

fn median(_values: Vec<f64>) -> f64 {
    0.0
}

fn p95(_values: Vec<f64>) -> f64 {
    0.0
}

fn evaluate(_kpi: Kpi, _value: f64, _target: f64, _baseline: Option<f64>) -> Status {
    Status::Pass
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
    assert_eq!(
        evaluate(Kpi::SftpRatio, 0.79, 0.8, Some(1.0)),
        Status::Fail
    );
}

#[tokio::test]
#[ignore = "solo se ejecuta con just kpi"]
async fn kpi_benchmarks() {
    panic!("benchmark pendiente de implementar");
}
