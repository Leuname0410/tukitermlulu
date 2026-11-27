//! Ocultación de la salida de la línea de integración hasta el marcador listo.

use tuki_application::shell_integration::{READY_MARKER, READY_MARKER_ST};

/// Máximo de bytes retenidos mientras esperamos el marcador (256 KiB).
const MAX_BUFFER: usize = 256 * 1024;

/// Filtro puro: fuera de la ventana pasa bytes; dentro retiene hasta el marcador.
#[derive(Default)]
pub struct EchoFilter {
    #[allow(dead_code)]
    active: bool,
    #[allow(dead_code)]
    buffer: Vec<u8>,
}

impl EchoFilter {
    /// Crea un filtro inactivo.
    pub fn new() -> Self {
        Self::default()
    }

    /// Empieza a retener salida hasta ver uno de los marcadores de `tuki-ready`.
    pub fn activate(&mut self) {
        self.active = true;
    }

    /// Devuelve los bytes visibles de un fragmento; oculta el resto.
    #[allow(dead_code)]
    pub fn push(&mut self, bytes: &[u8]) -> Vec<u8> {
        bytes.to_vec()
    }

    /// Reenvía lo retenido si no apareció el marcador antes del timeout.
    #[allow(dead_code)]
    pub fn expire(&mut self) -> Vec<u8> {
        self.active = false;
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{EchoFilter, MAX_BUFFER, READY_MARKER, READY_MARKER_ST};

    #[test]
    fn should_return_only_bytes_after_marker_when_marker_is_in_a_chunk() {
        let mut filter = EchoFilter::new();
        filter.activate();
        let mut chunk = b"echo ".to_vec();
        chunk.extend_from_slice(READY_MARKER);
        chunk.extend_from_slice(b"\r\nprompt$ ");
        assert_eq!(filter.push(&chunk), b"\r\nprompt$ ".to_vec());
        assert_eq!(filter.push(b"ls\n"), b"ls\n".to_vec());
    }

    #[test]
    fn should_ignore_bytes_before_marker_when_marker_is_split_across_chunks() {
        let mut filter = EchoFilter::new();
        filter.activate();
        assert!(filter.push(b"echo \x1b]777;tuki-re").is_empty());
        assert_eq!(filter.push(b"ady\x07resto"), b"resto".to_vec());
    }

    #[test]
    fn should_accept_st_terminator_when_cmd_emits_the_marker() {
        let mut filter = EchoFilter::new();
        filter.activate();
        assert!(filter.push(b"\x1b]777;tuki-ready\x1b").is_empty());
        assert_eq!(filter.push(b"\\prompt> "), b"prompt> ".to_vec());
    }

    #[test]
    fn should_pass_marker_through_when_filter_is_inactive() {
        let mut filter = EchoFilter::new();
        let mut chunk = READY_MARKER.to_vec();
        chunk.extend_from_slice(b"prompt$ ");
        assert_eq!(filter.push(&chunk), chunk);
        assert_eq!(filter.push(READY_MARKER_ST), READY_MARKER_ST.to_vec());
    }

    #[test]
    fn should_return_everything_kept_when_expiring_without_a_marker() {
        let mut filter = EchoFilter::new();
        filter.activate();
        assert!(filter.push(b"guardado").is_empty());
        assert_eq!(filter.expire(), b"guardado".to_vec());
        assert!(filter.expire().is_empty());
    }

    #[test]
    fn should_stop_buffering_when_the_cap_is_exceeded() {
        let mut filter = EchoFilter::new();
        filter.activate();
        let chunk = vec![b'x'; MAX_BUFFER];
        assert!(filter.push(&chunk).is_empty());
        let flushed = filter.push(b"y");
        assert_eq!(flushed.len(), MAX_BUFFER + 1);
        assert_eq!(filter.push(READY_MARKER), READY_MARKER.to_vec());
    }
}
