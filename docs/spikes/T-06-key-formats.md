# T-06 — Spike: formatos de llave que carga `russh`

Versiones: `russh 0.65.4` (fija `ssh-key 0.7.0-rc.12` con `ppk` + `encryption`), `ring 0.17.14`, Rust 1.100.
API: `russh::keys::load_secret_key(path, password)`. `ssh-key` directo: no probado, ningún formato falló (plan B innecesario).
Passphrase correcta `tuki-pass`, incorrecta `wrong-pass`. 18 tests, 0 ignorados, sin red.

| Llave | russh | ssh-key | error con passphrase mala (variante / mensaje) |
|---|---|---|---|
| `ed25519` (OpenSSH) | OK | n/a | — (sin passphrase) |
| `keys/rsa_openssh` (OpenSSH RSA) | OK | n/a | — (sin passphrase) |
| `keys/rsa_openssh_pass` (OpenSSH RSA) | OK | n/a | `SshKey(Crypto)` / `SshKey: cryptographic error` |
| `keys/rsa_pkcs1.pem` (PEM PKCS#1) | OK | n/a | — (sin passphrase) |
| `keys/rsa_pkcs8.pem` (PEM PKCS#8) | OK | n/a | — (sin passphrase) |
| `keys/ecdsa_p256` (OpenSSH ECDSA) | OK | n/a | — (sin passphrase) |
| `keys/ecdsa_p256_pass` (OpenSSH ECDSA) | OK | n/a | `SshKey(Crypto)` / `SshKey: cryptographic error` |
| `keys/ed25519_pass` (OpenSSH Ed25519) | OK | n/a | `SshKey(Crypto)` / `SshKey: cryptographic error` |
| `keys/ed25519_v2.ppk` (PPK v2) | OK | n/a | — (sin passphrase) |
| `keys/ed25519_v2_pass.ppk` (PPK v2) | OK | n/a | `SshKey(Ppk(IncorrectMac))` / `SshKey: PPK parsing error: incorrect MAC` |
| `keys/ed25519_v3.ppk` (PPK v3) | OK | n/a | — (sin passphrase) |
| `keys/ed25519_v3_pass.ppk` (PPK v3 Argon2id) | OK | n/a | `SshKey(Ppk(IncorrectMac))` / `SshKey: PPK parsing error: incorrect MAC` |
| `keys/rsa_v3.ppk` (PPK v3 RSA) | OK | n/a | — (sin passphrase) |

Los errores con passphrase mala son genéricos: no revelan la llave ni la passphrase.
En Windows el backend por defecto (`aws-lc-rs`) exige NASM; el spike usa `default-features = false` con `ring` + `rsa`, sin efecto en la carga de llaves.

Recomendación: usar `russh::keys::load_secret_key` como base del `KeyLoader` (T-09), solicitando passphrase solo a llaves cifradas y sin persistirla.
