# Fixtures de prueba SSH

**Solo para tests.** Nunca usar estas llaves fuera del entorno de pruebas.

Contenido:
- `ed25519` / `ed25519.pub`: llave del cliente (comentario `tuki-test`), sin passphrase.
- `host/ssh_host_ed25519_key` / `.pub`: llave host fija del servidor.
- `known_hosts`: entrada para `[127.0.0.1]:2222` con la llave host fija.
- `Dockerfile` / `docker-compose.yml`: servidor OpenSSH reproducible.

Usuario: `tuki`. Contraseña: `tuki-test`. Puerto: `2222` (solo en `127.0.0.1`).

Levantar: `docker compose -f tests/fixtures/docker-compose.yml up -d --build`
Bajar: `docker compose -f tests/fixtures/docker-compose.yml down`

Verificación de host activa (no desactivar `StrictHostKeyChecking`):
`ssh -i tests/fixtures/ed25519 -p 2222 -o UserKnownHostsFile=tests/fixtures/known_hosts -o StrictHostKeyChecking=yes -o IdentitiesOnly=yes tuki@127.0.0.1 echo ok`

**No regenerar** este material. Si se regenera, actualizar `known_hosts` con la nueva llave host.
