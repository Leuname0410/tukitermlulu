# ADR-003: Carga de llaves privadas con `russh` (resultado del spike T-06)

**Estado:** Propuesto (pendiente de aprobación humana) · **Fecha:** 2026-11-26

## Contexto
El plan exigía confirmar, antes del `KeyLoader` (T-09), que `russh` carga PEM (OpenSSH / PKCS#1 / PKCS#8) y PPK v2/v3 con Argon2; plan B: `ssh-key` con la feature `ppk`. Informe: `docs/spikes/T-06-key-formats.md`.

## Decisión
- `KeyLoader` se implementa sobre `russh::keys::load_secret_key` (russh 0.65.x). Las 13 llaves de fixture cargan: RSA/ECDSA/Ed25519 OpenSSH (con y sin passphrase), PEM PKCS#1 y PKCS#8, PPK v2 y v3 (Argon2id) con y sin passphrase. **No se necesita el plan B.**
- Mapeo de errores en T-09: `SshKey(Crypto)` (OpenSSH) y `SshKey(Ppk(IncorrectMac))` (PPK) → `KeyError::WrongPassphrase`; formato no reconocido → `KeyError::UnsupportedFormat`. Nunca se propaga el mensaje original a la UI.
- **Backend criptográfico: `ring`** (`default-features = false, features = ["rsa", "ring"]`). El backend por defecto, `aws-lc-rs`, requiere NASM (y CMake) para compilar en Windows, lo que complica el entorno de desarrollo y el CI.

## Alternativas descartadas
- **`ssh-key` directo:** innecesario; `russh` ya lo integra (con `ppk` + `encryption`).
- **`aws-lc-rs`:** más algoritmos y validación FIPS, pero toolchain extra en Windows. Reevaluar si algún algoritmo requerido por un servidor no está en `ring`.

## Consecuencias
- (+) Un solo crate para SSH y llaves; cero dependencias extra.
- (−) `russh 0.65` depende de `ssh-key 0.7.0-rc.12` (release candidate): fijar versión en `Cargo.lock` y vigilar `cargo audit`.
- (−) Con `ring` hay que verificar en T-13 que los KEX/cifrados negociados con OpenSSH y Windows OpenSSH Server son suficientes.
