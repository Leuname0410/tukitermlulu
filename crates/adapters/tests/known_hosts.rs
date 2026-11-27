//! Pruebas de integración del verificador TOFU sobre known_hosts propio.

use tempfile::tempdir;
use tuki_adapters::known_hosts::FileHostKeyVerifier;
use tuki_core::host::{Host, Hostname, Port, Username};
use tuki_core::port_errors::HostKeyError;
use tuki_core::ports::{HostKeyFingerprint, HostKeyStatus, HostKeyVerifier};

fn host(name: &str, port: u16) -> Host {
    Host {
        hostname: Hostname::new(name).expect("hostname válido"),
        port: Port::new(port).expect("puerto válido"),
        username: Username::new("tester").expect("usuario válido"),
    }
}

fn fingerprint(value: &str) -> HostKeyFingerprint {
    HostKeyFingerprint::new(value).expect("huella válida")
}

async fn status(verifier: &FileHostKeyVerifier, host: &Host, value: &str) -> HostKeyStatus {
    verifier
        .verify(host, &fingerprint(value))
        .await
        .expect("verificación")
}

#[tokio::test]
async fn should_return_known_when_trusting_a_new_host() {
    let dir = tempdir().expect("directorio temporal");
    let path = dir.path().join("nested").join("known_hosts");
    let verifier = FileHostKeyVerifier::new(&path);
    let host = host("example.com", 22);

    assert_eq!(
        status(&verifier, &host, "SHA256:abc").await,
        HostKeyStatus::Unknown
    );
    assert_eq!(
        verifier.trust(&host, &fingerprint("SHA256:abc")).await,
        Ok(())
    );
    assert_eq!(
        status(&verifier, &host, "SHA256:abc").await,
        HostKeyStatus::Known
    );

    let content = tokio::fs::read_to_string(&path)
        .await
        .expect("archivo creado");
    assert!(content.contains("[example.com]:22 SHA256:abc"));
}

#[tokio::test]
async fn should_return_changed_when_fingerprint_differs_for_the_same_host() {
    let dir = tempdir().expect("directorio temporal");
    let verifier = FileHostKeyVerifier::new(dir.path().join("known_hosts"));
    let host = host("example.com", 22);

    verifier
        .trust(&host, &fingerprint("SHA256:old"))
        .await
        .expect("primera confianza");

    assert_eq!(
        status(&verifier, &host, "SHA256:new").await,
        HostKeyStatus::Changed
    );
}

#[tokio::test]
async fn should_return_unknown_when_same_host_is_trusted_on_another_port() {
    let dir = tempdir().expect("directorio temporal");
    let verifier = FileHostKeyVerifier::new(dir.path().join("known_hosts"));

    verifier
        .trust(&host("example.com", 22), &fingerprint("SHA256:abc"))
        .await
        .expect("confianza en el puerto 22");

    assert_eq!(
        status(&verifier, &host("example.com", 2200), "SHA256:abc").await,
        HostKeyStatus::Unknown
    );
}

#[tokio::test]
async fn should_resolve_the_same_entry_when_hostname_case_differs() {
    let dir = tempdir().expect("directorio temporal");
    let verifier = FileHostKeyVerifier::new(dir.path().join("known_hosts"));

    verifier
        .trust(&host("Example.COM", 22), &fingerprint("SHA256:abc"))
        .await
        .expect("confianza con mayúsculas");

    assert_eq!(
        status(&verifier, &host("example.com", 22), "SHA256:abc").await,
        HostKeyStatus::Known
    );
    assert_eq!(
        status(&verifier, &host("EXAMPLE.com", 22), "SHA256:other").await,
        HostKeyStatus::Changed
    );
}

#[tokio::test]
async fn should_keep_a_single_line_when_trusting_a_changed_entry() {
    let dir = tempdir().expect("directorio temporal");
    let path = dir.path().join("known_hosts");
    let verifier = FileHostKeyVerifier::new(&path);
    let host = host("example.com", 22);

    verifier
        .trust(&host, &fingerprint("SHA256:old"))
        .await
        .expect("primera confianza");
    verifier
        .trust(&host, &fingerprint("SHA256:new"))
        .await
        .expect("reemplazo");

    let content = tokio::fs::read_to_string(&path)
        .await
        .expect("archivo leído");
    let entries: Vec<_> = content
        .lines()
        .filter(|line| line.starts_with("[example.com]:22"))
        .collect();
    assert_eq!(entries, vec!["[example.com]:22 SHA256:new"]);
    assert_eq!(
        status(&verifier, &host, "SHA256:new").await,
        HostKeyStatus::Known
    );
}

#[tokio::test]
async fn should_ignore_comments_and_malformed_lines_when_verifying() {
    let dir = tempdir().expect("directorio temporal");
    let path = dir.path().join("known_hosts");
    let content = "# comentario\n\nnot-an-entry\n[bad]:not-a-port SHA256:x\n[example.com]:22\n\
                   [example.com]:2222 SHA256:for-other-port\n[other]:22 SHA256:for-other-host\n\
                   [example.com]:22 SHA256:good\n";
    tokio::fs::write(&path, content)
        .await
        .expect("archivo sembrado");
    let verifier = FileHostKeyVerifier::new(&path);

    assert_eq!(
        status(&verifier, &host("example.com", 22), "SHA256:good").await,
        HostKeyStatus::Known
    );
    assert_eq!(
        status(&verifier, &host("example.com", 22), "SHA256:other").await,
        HostKeyStatus::Changed
    );
    assert_eq!(
        status(
            &verifier,
            &host("example.com", 2200),
            "SHA256:for-other-port"
        )
        .await,
        HostKeyStatus::Unknown
    );
    assert_eq!(
        status(&verifier, &host("other", 22), "SHA256:for-other-host").await,
        HostKeyStatus::Known
    );
}

#[tokio::test]
async fn should_reject_fingerprint_without_prefix_when_verifying_or_trusting() {
    let dir = tempdir().expect("directorio temporal");
    let path = dir.path().join("known_hosts");
    let verifier = FileHostKeyVerifier::new(&path);
    let host = host("example.com", 22);

    assert_eq!(
        verifier.verify(&host, &fingerprint("deadbeef")).await,
        Err(HostKeyError::InvalidFingerprint)
    );
    assert_eq!(
        verifier.trust(&host, &fingerprint("deadbeef")).await,
        Err(HostKeyError::InvalidFingerprint)
    );
    assert!(!path.exists());
}
