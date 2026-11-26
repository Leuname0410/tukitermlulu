# Progreso

Generador: `opencode-cli` 2.4.2 (`%APPDATA%\ai.opencode.desktop\cli\2.4.2-x64\opencode-cli.exe`).
El `opencode` 1.19.6 del PATH (npm) no puede abrir la DB ya migrada por la 2.4.2.

| ticket | estado | rondas | archivos | notas |
|---|---|---|---|---|
| T-01a | APPROVE | 2 | 4 (+lock) | rama `feat/T-01a-workspace-core` desde `main`. Paquetes `tuki-*` (Cargo prohíbe `core`). R1: licencia MIT→MPL-2.0 (error del ticket) |
| T-01b | APPROVE | 1 | 4 (+lock) | rama `feat/T-01b-app-adapters` desde T-01a. MINOR: warnings `unused dependency` en crates vacíos |
| T-01c | APPROVE | 1 | 5 (+lock) | rama `feat/T-01c-ui-scaffold` desde T-01b. React 19, Vite 8, TS 7 |
| T-01d | APPROVE | 2 | 6 (+iconos, locks) | rama `feat/T-01d-desktop-tauri` desde T-01c. R1: identifier `.app` + iconos móviles. CLI requiere `TAURI_APP_PATH=../crates/desktop`. Ventana verificada en Windows; Linux/macOS pendientes (CI/manual). MINOR: ignorar `crates/desktop/gen/` |
| T-02 | APPROVE | 1 | 5 (+4 manifiestos `publish`, locks) | rama `feat/T-02-justfile-gate` desde T-01d. Gate: 2 intentos (MSRV 1.85 retenía `time`/`quick-xml` vulnerables → 1.100 + `cargo update`). TS 7→5.9 por typescript-eslint. Avisos sin parche (no ignorados): `chrono`, `proc-macro-error` (unmaintained), `glib 0.18` (unsound, Linux/GTK vía Tauri) |
| T-03 | APPROVE | 1 | 1 | rama `feat/T-03-ci` desde T-02. macOS solo en PR (matriz `fromJSON`). Pendiente humano: checks verdes en los 3 SO tras push. MINOR: push+PR duplican ejecución. 1 cuelgue de OpenCode (relanzado) |
| T-04 | APPROVE (docs, sin generador) | — | 6 | rama `feat/T-04-docs` desde T-03. `AGENTS.md` 73 líneas, `CLAUDE.md` 59. `tickets/TEMPLATE.md`. ADR-001/002 en estado **Propuesto** (requieren tu aprobación). Pendiente: `ServerMetricsProvider` en AGENTS.md no está en el plan |
| T-05a | APPROVE | 1 | 5 (+llaves) | rama `feat/T-05a-ssh-fixtures` desde T-04. Alpine 3.22 + OpenSSH, `127.0.0.1:2222`, host key fija + `known_hosts`, `.gitattributes` LF. `ssh ... echo ok` verificado con StrictHostKeyChecking=yes |
| T-05b | APPROVE | 1 | 3 (+24 llaves) | rama `feat/T-05b-key-fixtures` desde T-05a. 12 llaves (OpenSSH/PKCS#1/PKCS#8 RSA, ECDSA P-256, Ed25519, PPK v2/v3 Argon2id), passphrase `tuki-pass`, generadas por `gen-keys.sh` en contenedor. Añadido `Subsystem sftp` (pendiente de T-05a). ssh con 3 formatos + sftp verificados |

| T-06 | APPROVE | 1 | 3 (+lock) | rama `feat/T-06-key-format-spike` desde `main` (22049eb). russh 0.65.4 carga las 13 llaves, sin plan B. Backend `ring` (aws-lc-rs exige NASM en Windows). ADR-003 propuesto |
| T-07a | APPROVE | 2 | 5 (+lock) | rama `feat/T-07a-host-value-objects` desde T-06. 23 tests. R1 BLOCKER: `join` aceptaba `""`, `.` y `\` (path traversal en Windows; error del ticket). MINOR: `Hostname` acepta `user@host`/`host:22` |
| T-07b | APPROVE | 1 | 4 | rama `feat/T-07b-session-entities` desde T-07a. 14 tests. Passphrase no representable en el dominio. MINOR: campos `pub` en `Session`/`Folder` permiten saltar validación (revisar en T-18); validación de nombre duplicada ×3 |

## F1 (redactado, sin ejecutar)
T-06 (spike formatos de llave) · T-07a (errores, Host/Port/RemotePath) · T-07b (Auth/Session/Folder) · T-08 (puertos; **bloqueado** por decisión async-trait vs `impl Future`)
