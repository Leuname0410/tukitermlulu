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
| T-08a | APPROVE | 1 | 4 (+lock) | rama `feat/T-08-core-ports` desde T-07b. Opción B (`impl Future + Send`). `SshTransport`, `RemoteFileSystem`, `CwdTracker`, `SecretString`/`PrivateKey` (zeroize, Debug redactado, sin Clone/Serialize). Partido por tamaño (autorizado). MINOR: errores sin contexto; `PrivateKey` como bytes |
| T-08b | APPROVE | 1 | 2 | rama `feat/T-08b-remaining-ports` desde T-08a. `KeyLoader`, `SecretStore` (solo contraseñas, ADR-004), `HostKeyVerifier` (verify + trust TOFU), `SessionRepository`. Errores con mensajes literales. MINOR: huella no valida prefijo `SHA256:` |

| T-09 | APPROVE | 1 | 5 (+lock) | rama `feat/T-09-key-loader` desde `main` (297538c). 25 tests; spike borrado. MINOR: errores PPK clasificados por texto de `Debug` |
| T-10 | APPROVE | 1 | 4 (+lock) | rama `feat/T-10-secret-store` desde T-09. Gate 2 intentos (lockfile sin commitear por mi instrucción). Keyring real verificado en Windows |
| T-11 | APPROVE | 2 | 4 (+lock) | rama `feat/T-11-known-hosts` desde T-10. R1 MAJOR: huella sin validar permitía inyectar líneas. MINOR: temporal huérfano si falla `rename`; sin lock entre `trust` concurrentes |
| T-12a | APPROVE | 1 | 4 (+lock) | rama `feat/T-12a-import-key-use-case` desde T-11. Primer caso de uso + puerto `KeyFileStore`. MINOR: `has_passphrase=true` si se da passphrase a llave sin cifrar (→ T-18b) |
| T-12b | APPROVE | 2 | 4 (+lock) | rama `feat/T-12b-key-file-store` desde T-12a. R1 BLOCKER: candidatos leídos sin `Zeroizing`. Windows: ACL heredada de `%APPDATA%` (pendiente de validar). `cfg(unix)` solo verificable en CI |

| T-13a | APPROVE | 1 | 3 | rama `feat/T-13a-ssh-port-handshake` desde T-12b. ADR-005: `connect` → `Connected{id, fingerprint}` sin autenticar; `PtySink` síncrono |
| T-13b | APPROVE | 2 | 4 (+lock) | rama `feat/T-13b-russh-connect-auth` desde T-13a. R1 MAJOR: mutex global retenido en red; IPv6 con `format!` |
| T-13c | APPROVE | 1 | 2 | rama `feat/T-13c-russh-pty` desde T-13b. Espera fija de 1 s tras EOF para el exit status. MINOR: `write` copia bytes sin zeroize |
| T-14 | APPROVE | 1 | 3 (+lock) | rama `feat/T-14-connect-session` desde T-13c. Huella verificada antes de credenciales (`Sequence`). MINOR: `#[rustfmt::skip]`, líneas >150 |
| T-15a | APPROVE | 2 | 4 (+lock) | rama `feat/T-15a-tauri-terminal-bridge` desde T-14. R1 MAJOR: timeout externo dejaba conexiones huérfanas → movido al handshake del adaptador (20 s) |
| T-15b | APPROVE | 1 | 4 (+lock) | rama `feat/T-15b-ui-terminal-api` desde T-15a. MINOR: `fingerprint` tipado `string` pero llega `null`; salida no-ArrayBuffer se descarta en silencio |
| T-16 | APPROVE | 2 | 5 (+lock) | rama `feat/T-16-terminal-view` desde T-15b. R1 MAJOR: doble pegado sin `preventDefault`. Pendiente manual: vim/htop/sudo (tras T-17) |
| T-17a | APPROVE | 1 | 4 | rama `feat/T-17a-tab-passphrase` desde T-16. Passphrase en memoria del backend por pestaña (`TabPassphrases`); se borra con `WrongPassphrase`/`NeedsPassphrase`/`close_tab`. MINOR: `#[rustfmt::skip]` |
| T-17b | APPROVE | 1 | 5 | rama `feat/T-17b-tab-controller` desde T-17a. Reducer + `TabController` con buffer de 1 MiB previo al `attach`. Diff 523 líneas (272 tests) |
| T-17c | APPROVE | 2 | 6 (+4 del fix) | rama `feat/T-17c-tabs-ui` desde T-17b. R1 MAJOR: el PTY nunca recibía el tamaño real tras el fit inicial (raíz en `useTerminal.ts`, T-16). Prompts inline provisionales hasta T-20. MINOR: al terminar la sesión se desmonta la terminal y se pierde la salida |

| T-18 | APPROVE | 1 | 3 (+lock) | rama `feat/T-18-sqlite-repository` desde T-17c. `rusqlite` 0.43 `bundled` (0.35 con CVE), migraciones con `PRAGMA user_version`. MINOR: ciclo A→B→A entre carpetas no se impide |
| T-18b | APPROVE | 1 | 4 | rama `feat/T-18b-session-use-cases` desde T-18. Create/Update/Duplicate/Delete; `DuplicateSession` sin acceso a `SecretStore` |
| T-18c | APPROVE | 1 | 6 | rama `feat/T-18c-session-commands` desde T-18b. `sessions.db` en `app_data_dir`; `connect` por `sessionId`. MINOR: `create` valida e incrementa id dos veces; `list` lee el keyring por sesión |
| T-19 | APPROVE | 1 | 5 | rama `feat/T-19-session-tree` desde T-18c. `SessionTree.tsx` → `SessionTreeView.tsx` (colisión de mayúsculas en Windows). MINOR: carpetas en ciclo no se muestran |
| T-20 | APPROVE | 1 | 7 | rama `feat/T-20-dialogs` desde T-19. Autorizado tocar `App.tsx`/`TabsView.tsx` para el callback de actualizar contraseña. MINOR: la contraseña nueva espera en un `Map` de `App` hasta confirmar la conexión |

| T-21 | APPROVE | 1 | 4 (+lock) | rama `feat/T-21-sftp-adapter` desde T-20. `russh-sftp` 4.0.0 sobre la conexión de la terminal. MINOR: carrera al abrir la sesión SFTP deja un canal extra; caché no se limpia al cerrar |
| T-22a | APPROVE | 2 | 4 | rama `feat/T-22a-sftp-transfers` desde T-21. Puerto `FileTransfer`, bloques de 256 KiB. R1 MAJOR: `.part` truncaba archivos ajenos → creación exclusiva. Pendiente manual: 1 GB |
| T-22b | APPROVE | 1 | 2 | rama `feat/T-22b-transfer-queue` desde T-22a. Cola FIFO + `local_download_target` anti traversal. MINOR: `CON .txt` no se detecta |
| T-22c | APPROVE | 1 | 5 | rama `feat/T-22c-transfer-worker` desde T-22b. Worker único, evento `transfers` limitado a 100 ms. Diff ~880 líneas |
| T-23a | APPROVE | 1 | 3 | rama `feat/T-23a-sftp-home-tdd` (rehecha con `test:` antes de `feat:`; la rama `feat/T-23a-sftp-home` original sobra). MINOR: un test de integración falló una vez al conectar (posible flaky) |
| T-23b | APPROVE | 1 | 4 | rama `feat/T-23b-sftp-commands` desde T-23a. 5 comandos SFTP |
| T-23c | APPROVE | 1 | 5 | rama `feat/T-23c-sftp-sidebar` desde T-23b. Sidebar a la izquierda al conectar (D1) |
| T-24a | APPROVE | 1 | 5 | rama `feat/T-24a-drop-upload` desde T-23c. Soltar del SO → subir. T-24b (arrastrar al SO) fuera de v1 (opción b). MINOR: carpeta detectada por texto de error. Pendiente manual: Explorer/Finder/Linux |
| T-25 | APPROVE | 1 | 4 | rama `feat/T-25-split-pane` desde T-24a. Ancho por sesión en `localStorage` |
| T-26a | APPROVE | 1 | 7 | rama `feat/T-26a-sftp-chmod` desde T-25. `chmod` + `downloads_dir` |
| T-26b | APPROVE | 1 | 5 | rama `feat/T-26b-sftp-menu` desde T-26a. Menú contextual, permisos y lista de transferencias. 126 tests UI. MINOR: renombrar en la raíz construye una ruta relativa |

| T-27 | APPROVE | 1 | 2 | rama `feat/T-27-osc7-tracker` desde T-26b. Parser OSC 7 por estados, tope 4096 |
| T-28a | APPROVE | 1 | 6 | rama `feat/T-28a-cwd-channel` desde T-27. Canal `on_cwd`; salida enviada intacta antes de analizarla |
| T-28b | APPROVE | 2 | 5 | rama `feat/T-28b-shell-probe` desde T-28a. `exec` en el puerto SSH + sonda + líneas por shell. R1: 3 MAJOR (limpieza antes del marcador, `` `e `` no existe en PS 5.1, `$E` en `echo` de cmd) |
| T-28c | APPROVE | 1 | 6 (+5 ajustes) | rama `feat/T-28c-shell-flag` desde T-28b. `shell_integration` por sesión, SQLite v2 |
| T-28d | APPROVE | 2 | 6 | rama `feat/T-28d-shell-inject` desde T-28c. Inyección + `EchoFilter`; `bash`/`zsh`/`fish` en el contenedor. R1 MAJOR: salida desordenada al expirar. MINOR: la tarea espera los 2 s aunque llegue el marcador |
| T-30 | APPROVE | 1 | 5 | rama `feat/T-30-follow-terminal` desde T-28d. "Seguir terminal" + `RemotePath::from_terminal` |

| T-34a | APPROVE | 1 | 7 | rama `feat/T-34a-tracing-logs` desde T-30. `tracing` + rotación diaria (7) + `RedactingFields`. MINOR: diff ~650 líneas; formato `message=` |
| T-34b | APPROVE | 2 | 7 | rama `feat/T-34b-diagnostics` desde T-34a. Exportar diagnóstico a Descargas. R1: 2 MAJOR (I/O en el hilo principal, sin carpeta de logs) |
| T-33a | APPROVE | 1 | 3 | rama `feat/T-33a-bundle` desde T-34b. Bundle de 6 targets; Windows local: NSIS 4.49 MB, MSI 6.44 MB (KPI < 15 MB ✔) |

| T-33c | APPROVE | 2 | 2 | rama `feat/T-33c-release` desde T-33a. `release.yml` por tag (3 SO, borrador + SHA256SUMS); T-33b integrado (sin firma, D1/D2). R1 MAJOR: faltaba `TAURI_APP_PATH` |
| T-32a | APPROVE | 2 | 6 | rama `feat/T-32a-kpi`. KPIs de red con mediana de 5 y gate del 20 %. R1: `kpi` solo en tags y dispatch (ahorro de CI) |
| T-32c | APPROVE | 2 | 3 | rama `feat/T-32c-sftp-pipeline`. SFTP en pipeline (64 × 32 KiB). R1: corrupción si el remoto se encoge + `kpi` medía debug. SFTP 1,31× OpenSSH |
| T-34c | APPROVE | 1 | 5 | rama `feat/T-34c-ui-log`. `logUiError` → `log_ui_error`. MINOR: saltos de línea en el mensaje (log forging), para la auditoría |
| T-32b | APPROVE | 1 | 1 | rama `feat/T-32b-coverage`. `just coverage`: core+application 98,1 %, adapters 87,8 % |
| T-31a | APPROVE | 2 | 8 | rama `feat/T-31a-e2e`. e2e local en Windows. **Bug de producción encontrado: el CSP bloqueaba todo el IPC en release** (corregido + test). Arranque en frío ~390 ms |
| T-31d | APPROVE | 1 | 3 | rama `feat/T-31d-ram`. `--disable-gpu`: 285 → 177 MB. Meta Windows ≤ 185 MB (humano) |
| T-31c | APPROVE | 1 | 2 | rama `feat/T-31c-macos-smoke`. Smoke macOS bajo demanda + `docs/qa-macos.md`. MINOR: la RAM de macOS no cuenta los procesos XPC de WebKit |
| T-35 | APPROVE | 1 | 1 | rama `feat/T-35-readme`. README de portafolio; GIF pendiente (humano) |

T-31b: descartado. El e2e del cliente Windows está en T-31a; los servidores Windows van al checklist manual T-36.

## F7: cerrada (2026-11-27). Rama final `feat/T-35-readme`, sin merge a `main`.
Pendiente humano (T-36 y otros):
- QA manual en los 3 SO contra EC2 Linux/Windows: subida arrastrando, PowerShell 5.1/cmd, CPU con `--disable-gpu` (htop, scroll) y `docs/qa-macos.md`;
- tag `v0.1.0-rc.1` para probar `release.yml`;
- lanzar a mano el workflow CI (smoke de macOS, e2e de Linux y kpi);
- grabar `docs/media/demo.gif`;
- aprobar ADR-001 a 007.

Siguiente: F8 (`docs/F8-plan.md`), luego indexar con codegraph, la revisión de Claude sin contexto y la auditoría de logs; después, la preparación de la beta.

## F6: cerrada (2026-11-27)
T-27 → T-28a → T-28b → T-28c → T-28d → T-30 (T-29 descartado por ADR-006). `feat/T-30-follow-terminal` contiene F0–F6 (sin merge a `main`). ADR-006 sigue **Propuesto**.
Pendiente humano: `just dev` con `tuki-bash`/`tuki-zsh`/`tuki-fish` (`cd /var/log` → el sidebar sigue, sin eco visible; desmarcar → no sigue); PowerShell 5.1 y cmd.exe contra un Windows con OpenSSH.

## F5: cerrada (2026-11-26)
T-21 → T-22a → T-22b → T-22c → T-23a → T-23b → T-23c → T-24a → T-25 → T-26a → T-26b. `feat/T-26b-sftp-menu` contiene F0–F5 (sin merge a `main`).
Pendiente humano: `just dev`: sidebar al conectar, navegar, soltar archivos desde el SO, redimensionar, menú contextual (descargar a Descargas, renombrar, borrar, nueva carpeta, permisos), cancelar una transferencia, 1 GB sin crecer la memoria.
Siguiente: F6 según `docs/F6-plan.md` (ADR-006 pendiente de aprobación).

## F4: cerrada (2026-11-26)
T-18 → T-18b → T-18c → T-19 → T-20. `feat/T-20-dialogs` contiene F0–F4.
Pendiente humano: `just dev`: crear/editar/duplicar/borrar sesiones; conectar desde el árbol con "recordar"; pegar passphrase con Ctrl/Cmd+V y menú contextual; contraseña errónea guardada → diálogo con "Recordar" la actualiza.

## F3: cerrada (2026-11-26)
T-13a → T-13b → T-13c → T-14 → T-15a → T-15b → T-16 → T-17a → T-17b → T-17c. `feat/T-17c-tabs-ui` contiene F0–F3.
Pendiente humano: prueba manual con `just dev` contra `127.0.0.1:2222` (contraseña y llave con passphrase; vim, htop, pegar en `sudo`; `exit` + Reconectar sin pedir passphrase; cerrar y reabrir la pide).

## F2: cerrada (2026-11-26)
T-09 → T-10 → T-11 → T-12a → T-12b. `feat/T-12b-key-file-store` contiene F0 + F1 + F2.

## F1: cerrada (2026-11-26)
T-06 → T-07a → T-07b → T-08a → T-08b, encadenadas. `feat/T-08b-remaining-ports` contiene F0 + F1.
