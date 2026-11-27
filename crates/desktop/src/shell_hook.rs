//! Inyección de la integración OSC 7 tras la conexión, sin bloquear `connect`.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tuki_adapters::echo_filter::EchoFilter;
use tuki_application::shell_integration::{PROBE_COMMAND, ShellKind, classify, integration_line};
use tuki_core::ports::{ChannelId, ConnectionId, SshTransport};

/// Instante en que la salida del PTY estuvo por última vez; compartido con el sink.
#[derive(Clone)]
pub(crate) struct QuietClock {
    #[allow(dead_code)]
    last: Arc<Mutex<Instant>>,
}

impl QuietClock {
    /// Crea el reloj con el instante inicial indicado (inyectable en tests).
    pub(crate) fn new(now: Instant) -> Self {
        Self {
            last: Arc::new(Mutex::new(now)),
        }
    }

    /// Registra salida nueva.
    pub(crate) fn mark(&self, now: Instant) {
        let _ = now;
    }

    /// Comprueba si han pasado `threshold` sin salida desde el último `mark`.
    pub(crate) fn is_quiet(&self, now: Instant, threshold: Duration) -> bool {
        let _ = (now, threshold);
        false
    }

    /// Comprueba si se agotó el tiempo máximo de espera.
    pub(crate) fn timed_out(now: Instant, started: Instant, limit: Duration) -> bool {
        now.saturating_duration_since(started) >= limit
    }
}

/// Tiempos de la secuencia; los tests los acortan para no esperar.
#[derive(Clone, Copy)]
pub(crate) struct HookTiming {
    #[allow(dead_code)]
    pub(crate) quiet: Duration,
    #[allow(dead_code)]
    pub(crate) max_wait: Duration,
    #[allow(dead_code)]
    pub(crate) expire: Duration,
}

impl Default for HookTiming {
    fn default() -> Self {
        Self {
            quiet: Duration::from_millis(300),
            max_wait: Duration::from_secs(3),
            expire: Duration::from_secs(2),
        }
    }
}

/// Sonda, silencio, ocultación y escritura; devuelve la salida sin marcador.
#[allow(dead_code)]
pub(crate) async fn integrate<T: SshTransport>(
    transport: Arc<T>,
    connection: ConnectionId,
    channel: ChannelId,
    clock: QuietClock,
    filter: Arc<Mutex<EchoFilter>>,
    timing: HookTiming,
) -> Vec<u8> {
    let _ = (transport, connection, channel, clock, filter, timing);
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_be_ready_after_300ms_of_silence_and_not_before() {
        let start = Instant::now();
        let clock = QuietClock::new(start);
        let threshold = Duration::from_millis(300);
        assert!(!clock.is_quiet(start + Duration::from_millis(299), threshold));
        assert!(clock.is_quiet(start + threshold, threshold));

        let shared = clock.clone();
        shared.mark(start + threshold);
        assert!(!clock.is_quiet(start + Duration::from_millis(599), threshold));
        assert!(clock.is_quiet(start + Duration::from_millis(600), threshold));
    }

    #[test]
    fn should_stop_waiting_after_three_seconds_at_most() {
        let start = Instant::now();
        let max = Duration::from_secs(3);
        assert!(!QuietClock::timed_out(
            start + Duration::from_secs(2),
            start,
            max
        ));
        assert!(QuietClock::timed_out(start + max, start, max));
    }
}

#[allow(dead_code)]
fn _probe_references() -> (&'static str, fn(&tuki_core::ports::ExecOutput) -> ShellKind) {
    (PROBE_COMMAND, classify)
}

#[allow(dead_code)]
fn _line_reference(shell: ShellKind) -> Option<String> {
    integration_line(shell)
}
