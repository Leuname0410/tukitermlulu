//! Puente IPC de la terminal: DTOs, comandos y canal binario de salida.

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use tauri::ipc::{Channel, InvokeResponseBody};
    use tuki_core::port_errors::{HostKeyError, KeyError, SecretError, TransportError};
    use tuki_core::ports::{ChannelId, ConnectionId, HostKeyFingerprint, PtySize};

    use super::*;

    fn args() -> ConnectArgs {
        ConnectArgs {
            host: "example.com".to_owned(),
            port: 22,
            username: "alice".to_owned(),
            auth: AuthArgs::Password { remember: false },
            password: Some(Zeroizing::new("pw-secret".to_owned())),
            passphrase: Some(Zeroizing::new("pass-secret".to_owned())),
            approved_fingerprint: None,
            rows: 24,
            cols: 80,
        }
    }

    #[test]
    fn should_deliver_raw_bytes_and_close_status_when_pty_writes() {
        let output_bytes = Arc::new(Mutex::new(Vec::new()));
        let output = {
            let seen = Arc::clone(&output_bytes);
            Channel::new(move |body| {
                seen.lock().unwrap().push(body);
                Ok(())
            })
        };
        let exit_status = Arc::new(Mutex::new(None));
        let closed = {
            let seen = Arc::clone(&exit_status);
            Channel::new(move |body| {
                if let InvokeResponseBody::Json(json) = body {
                    let message: TerminalClosed = serde_json::from_str(&json).unwrap();
                    *seen.lock().unwrap() = message.exit_status;
                }
                Ok(())
            })
        };
        let mut sink = ChannelSink { output, closed };

        sink.data(&[1, 2, 3]);
        sink.closed(Some(0));

        let bodies = output_bytes.lock().unwrap();
        assert_eq!(bodies.len(), 1);
        match &bodies[0] {
            InvokeResponseBody::Raw(bytes) => assert_eq!(bytes.as_slice(), &[1, 2, 3]),
            other => panic!("cuerpo inesperado: {other:?}"),
        }
        assert_eq!(*exit_status.lock().unwrap(), Some(0));
    }

    #[test]
    fn should_reject_invalid_input_when_building_domain_request() {
        let mut invalid_port = args();
        invalid_port.port = 0;
        assert!(matches!(to_domain(&invalid_port), Err(error) if error.kind == "invalid_input"));

        let mut empty_host = args();
        empty_host.host.clear();
        assert!(matches!(to_domain(&empty_host), Err(error) if error.kind == "invalid_input"));

        let mut zero_rows = args();
        zero_rows.rows = 0;
        assert!(matches!(to_domain(&zero_rows), Err(error) if error.kind == "invalid_input"));

        let (session, size, password, passphrase, approved) = to_domain(&args()).unwrap();
        assert_eq!(session.host.port.get(), 22);
        assert_eq!(size, PtySize { rows: 24, cols: 80 });
        assert_eq!(password.map(|secret| secret.expose().to_owned()), Some("pw-secret".to_owned()));
        assert_eq!(passphrase.map(|secret| secret.expose().to_owned()), Some("pass-secret".to_owned()));
        assert!(approved.is_none());
    }

    #[test]
    fn should_map_every_connect_outcome_when_serializing_response() {
        let connected: ConnectResponse = ConnectOutcome::Connected {
            connection: ConnectionId::new(1),
            channel: ChannelId::new(2),
        }
        .into();
        assert_eq!(
            serde_json::to_string(&connected).unwrap(),
            r#"{"kind":"connected","connection_id":1,"channel_id":2}"#
        );
        let approval: ConnectResponse = ConnectOutcome::NeedsHostKeyApproval {
            fingerprint: HostKeyFingerprint::new("SHA256:abc").unwrap(),
        }
        .into();
        assert_eq!(
            serde_json::to_string(&approval).unwrap(),
            r#"{"kind":"needs_host_key_approval","fingerprint":"SHA256:abc"}"#
        );
        for (outcome, kind) in [
            (ConnectOutcome::NeedsPassword, "needs_password"),
            (ConnectOutcome::NeedsPassphrase, "needs_passphrase"),
        ] {
            let response: ConnectResponse = outcome.into();
            assert_eq!(
                serde_json::to_string(&response).unwrap(),
                format!(r#"{{"kind":"{kind}"}}"#)
            );
        }
    }

    #[test]
    fn should_map_domain_errors_when_connect_fails() {
        let kind = |error| CommandError::from_connect(error).kind;
        assert_eq!(
            kind(ConnectError::HostKeyChanged {
                fingerprint: HostKeyFingerprint::new("SHA256:abc").unwrap(),
            }),
            "host_key_changed"
        );
        assert_eq!(
            kind(ConnectError::Transport(TransportError::AuthenticationRejected)),
            "auth_rejected"
        );
        assert_eq!(
            kind(ConnectError::Transport(TransportError::NotConnected)),
            "not_connected"
        );
        assert_eq!(kind(ConnectError::Transport(TransportError::Connect)), "io");
        assert_eq!(
            kind(ConnectError::Key(KeyError::WrongPassphrase)),
            "wrong_passphrase"
        );
        assert_eq!(kind(ConnectError::Key(KeyError::NotFound)), "io");
        assert_eq!(kind(ConnectError::Secret(SecretError::Io)), "io");
        assert_eq!(kind(ConnectError::HostKey(HostKeyError::Io)), "io");
        assert_eq!(CommandError::timeout().kind, "timeout");
    }

    #[test]
    fn should_hide_arguments_secrets_when_serializing_responses() {
        let request = args();
        let response: ConnectResponse = ConnectOutcome::NeedsPassword.into();
        let error = CommandError::from_connect(ConnectError::Transport(TransportError::Io));
        let json = format!(
            "{}{}",
            serde_json::to_string(&response).unwrap(),
            serde_json::to_string(&error).unwrap()
        );
        assert!(!json.contains("pw-secret"));
        assert!(!json.contains("pass-secret"));
        let debug = format!("{:?}", request);
        assert!(!debug.contains("pw-secret"));
        assert!(!debug.contains("pass-secret"));
        assert!(debug.contains("[REDACTED]"));
    }
}
