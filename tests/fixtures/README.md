# Fixtures de prueba SSH

**Solo para tests.** Nunca usar estas llaves fuera del entorno de pruebas.

Contenido:
- `ed25519` / `ed25519.pub`: llave del cliente (comentario `tuki-test`), sin passphrase.
- `host/ssh_host_ed25519_key` / `.pub`: llave host fija del servidor.
- `known_hosts`: entrada para `[127.0.0.1]:2222` con la llave host fija.
- `Dockerfile` / `docker-compose.yml`: servidor OpenSSH reproducible.

Usuario: `tuki`. Contraseña: `tuki-test`. Puerto: `2222` (solo en `127.0.0.1`).
Passphrase de prueba para las llaves protegidas: `tuki-pass`.

Levantar: `docker compose -f tests/fixtures/docker-compose.yml up -d --build`
Bajar: `docker compose -f tests/fixtures/docker-compose.yml down`

Verificación de host activa (no desactivar `StrictHostKeyChecking`):
`ssh -i tests/fixtures/ed25519 -p 2222 -o UserKnownHostsFile=tests/fixtures/known_hosts -o StrictHostKeyChecking=yes -o IdentitiesOnly=yes tuki@127.0.0.1 echo ok`

## Llaves de cliente

Todas las `.pub` están en formato OpenSSH, con comentario `tuki-test-<nombre>`.
Las llaves `keys/*` están autorizadas en el contenedor junto a `ed25519.pub`.

| Archivo | Formato | Passphrase |
|---|---|---|
| `ed25519` | OpenSSH Ed25519 | sin passphrase |
| `keys/rsa_openssh` | OpenSSH RSA 3072 | sin passphrase |
| `keys/rsa_openssh_pass` | OpenSSH RSA 3072 | `tuki-pass` |
| `keys/rsa_pkcs1.pem` | PEM PKCS#1, RSA 3072 | sin passphrase |
| `keys/rsa_pkcs8.pem` | PEM PKCS#8, RSA 3072 | sin passphrase |
| `keys/ecdsa_p256` | OpenSSH ECDSA P-256 | sin passphrase |
| `keys/ecdsa_p256_pass` | OpenSSH ECDSA P-256 | `tuki-pass` |
| `keys/ed25519_pass` | OpenSSH Ed25519 | `tuki-pass` |
| `keys/ed25519_v2.ppk` | PPK v2, Ed25519 | sin passphrase |
| `keys/ed25519_v2_pass.ppk` | PPK v2, Ed25519 | `tuki-pass` |
| `keys/ed25519_v3.ppk` | PPK v3, Ed25519 | sin passphrase |
| `keys/ed25519_v3_pass.ppk` | PPK v3, Ed25519 (Argon2id) | `tuki-pass` |
| `keys/rsa_v3.ppk` | PPK v3, RSA 3072 | sin passphrase |

Las llaves PPK y sus `.pub` se generaron con `puttygen` dentro de un contenedor;
`gen-keys.sh` documenta y reproduce el proceso. Las v3 protegidas usan
`Key-Derivation: Argon2id` en la cabecera.

Verificación adicional:
`ssh -i tests/fixtures/keys/ecdsa_p256 -p 2222 -o UserKnownHostsFile=tests/fixtures/known_hosts -o StrictHostKeyChecking=yes -o IdentitiesOnly=yes tuki@127.0.0.1 echo ok`
`printf 'ls\n' | sftp -b - -i tests/fixtures/ed25519 -P 2222 -o UserKnownHostsFile=tests/fixtures/known_hosts -o StrictHostKeyChecking=yes tuki@127.0.0.1`

**No regenerar** este material. Si se regenera, actualizar `known_hosts` con la nueva llave host.
`gen-keys.sh` aborta si `keys/` ya existe.
