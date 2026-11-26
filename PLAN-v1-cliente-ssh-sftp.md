# Plan de trabajo v1 — Cliente SSH/SFTP open source inspirado en MobaXterm

> Proyecto de portafolio y uso personal. Diferenciador: **la UI de MobaXterm** — sidebar SFTP que sigue al directorio de la terminal, junto a la sesión SSH, sobre una sola conexión.

---

## 1. Objetivo y alcance

### Incluido en v1 (MVP)
- Gestor de sesiones (árbol con carpetas): host, puerto, usuario, método de auth, llave.
- Autenticación: llave **PEM** (OpenSSH / PKCS#1 / PKCS#8), llave **PPK** (v2 y v3), contraseña, passphrase.
- Terminal SSH con pestañas (xterm.js).
- **Sidebar SFTP** a la izquierda de la terminal: listar, subir, descargar, renombrar, borrar, crear carpeta, drag & drop, barra de progreso.
- **"Follow terminal folder"**: el SFTP navega al `cwd` de la shell remota.
- Servidores destino: Linux (bash/zsh) y Windows (OpenSSH Server + PowerShell/cmd).
- Plataformas cliente: **Linux, Windows y macOS** (Intel y Apple Silicon).

### Fuera de v1
X11, RDP/VNC, túneles, terminal local, multiexec, macros, sync entre dispositivos, editor remoto.

---

## 2. Stack fijo (no se discute en los tickets)

| Capa | Tecnología |
|---|---|
| Shell de escritorio | Tauri 2 |
| Backend | Rust stable, Tokio, `thiserror`, `tracing`, `serde` |
| SSH / SFTP | `russh`, `russh-sftp` |
| Persistencia | SQLite (`sqlx` o `rusqlite`), migraciones versionadas |
| Secretos | `keyring` (Credential Manager / Secret Service / Keychain de macOS), `zeroize` |
| Frontend | React + TypeScript + Vite, Zustand, Tailwind |
| Terminal | xterm.js + `@xterm/addon-fit` + `@xterm/addon-webgl` |
| Tests Rust | `cargo test`, `mockall`, `testcontainers` (OpenSSH en Docker) |
| Tests UI | Vitest + Testing Library |
| E2E | WebdriverIO + `tauri-driver` en Linux y Windows. En macOS no hay WebDriver para WKWebView: smoke test automatizado de arranque + checklist manual |
| CI | GitHub Actions (matriz ubuntu-latest / windows-latest / macos-latest) |
| WebViews | WebView2 (Windows), WebKitGTK (Linux), WKWebView (macOS): probar la UI en los tres |
| Calidad | `clippy -D warnings`, `rustfmt`, `cargo audit`, `cargo deny`, ESLint, Prettier |

---

## 3. Arquitectura (hexagonal / puertos y adaptadores)

```
repo/
├─ crates/
│  ├─ core/          # Dominio puro: entidades, value objects, traits (puertos). Sin I/O.
│  ├─ application/   # Casos de uso. Depende solo de `core`.
│  ├─ adapters/      # russh, russh-sftp, keyring, sqlite, known_hosts. Implementa los traits.
│  └─ desktop/       # Tauri: comandos (control) + channels (datos de terminal). Composition root.
├─ ui/               # React + TS
├─ tests/fixtures/   # llaves de prueba (PPK v2/v3, PEM RSA/Ed25519/ECDSA), docker-compose
├─ docs/adr/         # Architecture Decision Records
├─ AGENTS.md         # Contexto para OpenCode (generador)
└─ CLAUDE.md         # Contexto para Claude Code (revisor)
```

### Puertos (traits en `core`)
- `SshTransport` — conectar, autenticar, abrir canal PTY, resize, cerrar.
- `RemoteFileSystem` — list, stat, read/write stream, rename, remove, mkdir.
- `KeyLoader` — cargar llave privada desde archivo + passphrase → `PrivateKey`.
- `SecretStore` — guardar / leer / borrar secretos.
- `HostKeyVerifier` — verificar huella (TOFU, known_hosts).
- `SessionRepository` — CRUD de sesiones y carpetas.
- `CwdTracker` — obtener/observar el directorio actual de la shell.

### Reglas
- **SOLID**: un caso de uso por struct (SRP); los casos de uso dependen de traits (DIP); los protocolos nuevos son adaptadores nuevos (OCP).
- **Plano de control vs plano de datos**: `invoke` para acciones; Tauri **channels** binarios para el stream de la terminal. La salida de la terminal **nunca** pasa por el estado de React.
- Errores tipados por capa (`thiserror`); nada de `unwrap`/`expect` fuera de los tests.
- Secretos: nunca en SQLite, logs ni mensajes de error; `zeroize` al soltarlos.

---

## 4. Flujo multi-agente (Orca + OpenCode + Claude Code)

### Roles
| Rol | Agente | Responsabilidad |
|---|---|---|
| Orquestador | Orca | Toma el siguiente ticket, reparte el contexto mínimo y encadena los pasos |
| Generador | OpenCode | Escribe primero el test que falla (rojo), luego la implementación (verde) y refactoriza |
| Gate automático | CI local (`just check`) | fmt, clippy, tests, audit, lint UI |
| Revisor | Claude Code | Revisa **solo el diff + el ticket** con un checklist; aprueba o devuelve hallazgos |
| Humano | Tú | Decisiones de arquitectura (ADR), merge final y pruebas manuales contra servidores reales |

### Ciclo por ticket
1. Orca entrega a OpenCode el **ticket**, `AGENTS.md` y **solo los archivos listados** en el ticket.
2. OpenCode: test rojo → implementación → refactor → `just check`.
3. Si `just check` falla, vuelve a OpenCode **sin pasar por el revisor** (así no se gastan tokens de revisión).
4. Claude Code recibe `git diff`, el ticket y `CLAUDE.md`, y responde `APPROVE` o una lista numerada de hallazgos (severidad + archivo:línea).
5. Máximo **2 rondas** de revisión; a la tercera, escala a ti.
6. Haces el merge y cierras el ticket.

### Reglas para ahorrar tokens
- Tickets de **≤ 300 líneas de diff** y **≤ 5 archivos**. Si es más grande, se parte.
- Cada ticket lista los archivos a tocar y las firmas de las interfaces: el generador no explora el repo.
- `AGENTS.md` y `CLAUDE.md` de **menos de 150 líneas**: stack, convenciones, comandos y prohibiciones.
- El revisor no ejecuta tests (ya los pasó el gate); revisa diseño, seguridad y cobertura de criterios.
- Fixtures y docker-compose reutilizables para que ningún agente los regenere.

### Plantilla de ticket
```markdown
## T-XX: <título imperativo>
**Fase:** Fx  **Depende de:** T-YY
**Contexto (1–3 líneas):**
**Archivos a crear/modificar:** crates/core/src/..., ...
**Interfaces / firmas esperadas:**
**Criterios de aceptación (verificables por test):**
- [ ] ...
**Tests requeridos:** unit | integración | UI | e2e
**Fuera de alcance:**
```

### Checklist del revisor (en `CLAUDE.md`)
1. ¿Cada criterio de aceptación tiene un test que lo cubre?
2. ¿Respeta las capas? (`core` sin I/O; `application` solo usa traits)
3. ¿Hay `unwrap`/`expect`/`panic` o indexado sin chequear fuera de los tests?
4. ¿Se filtran secretos (logs, errores, serialización, `Debug`)?
5. ¿Hay `zeroize` sobre passphrases y material de llaves?
6. ¿Errores tipados y mensajes útiles para el usuario?
7. ¿Hay bloqueo del runtime de Tokio (I/O síncrono en contexto async)?
8. ¿Hay nombres, duplicación o responsabilidades mezcladas (SRP)?

---

## 5. Fases y tickets

> Estimaciones para dedicación de tiempo parcial (~10 h/semana). Total v1: **~13–15 semanas** (macOS añade ~1 semana, sobre todo en empaquetado y QA).

### F0 — Fundaciones (semana 1)
| ID | Ticket | Criterio clave |
|---|---|---|
| T-01 | Scaffold de Tauri 2 + React/TS + workspace de Cargo con los 4 crates | `just dev` abre una ventana vacía en Linux, Windows y macOS |
| T-02 | `justfile` con `check` (fmt, clippy, test, audit, deny, eslint, vitest) | `just check` en verde en un repo vacío |
| T-03 | CI en GitHub Actions con matriz Linux/Windows/macOS + cache | El PR muestra checks en verde en los tres SO |
| T-04 | `AGENTS.md`, `CLAUDE.md`, plantilla de ticket, ADR-001 (stack) y ADR-002 (hexagonal) | Ambos archivos de contexto con menos de 150 líneas |
| T-05 | docker-compose con OpenSSH (usuario/llaves de prueba) + fixtures de llaves | `ssh -i fixtures/ed25519` conecta al contenedor |

### F1 — Spike técnico y dominio (semana 2)
| ID | Ticket | Criterio clave |
|---|---|---|
| T-06 | **Spike**: comprobar que `russh` carga PPK v2/v3 (con/sin passphrase, Argon2) y PEM RSA/Ed25519/ECDSA | Informe en ADR-003; si falta un formato, plan B: crate `ssh-key` con la feature `ppk` |
| T-07 | Entidades del dominio: `Host`, `Session`, `Folder`, `AuthMethod`, `KeyRef`, `RemotePath` con validaciones | Tests unitarios de invariantes (puerto 1–65535, host no vacío, etc.) |
| T-08 | Definir traits/puertos en `core` + errores de dominio | Compila; casos de uso mockeables con `mockall` |

### F2 — Autenticación y llaves (semanas 3–4)
| ID | Ticket | Criterio clave |
|---|---|---|
| T-09 | Adaptador `KeyLoader`: PEM y PPK, detección de formato, errores claros (passphrase incorrecta, formato no soportado) | Test por fixture; el passphrase se borra de memoria con `zeroize` |
| T-10 | Adaptador `SecretStore` con `keyring` (Credential Manager, Secret Service, Keychain) + fake en memoria para tests | Ida y vuelta guardar/leer/borrar en los 3 SO; nada en disco en claro |
| T-11 | `HostKeyVerifier`: TOFU + archivo known_hosts propio; bloqueo si la llave cambia. Entradas indexadas por `[host]:puerto` | Tests: host nuevo → pregunta; mismo → ok; distinto → error; mismo host con otro puerto → host nuevo |
| T-12 | Caso de uso `ImportKey` (copia opcional a un directorio de la app con permisos 0600 en Linux/macOS / ACL de usuario en Windows) | Test unit con mocks + integración en Linux y macOS |

### F3 — Conexión SSH y terminal (semanas 5–6)
| ID | Ticket | Criterio clave |
|---|---|---|
| T-13 | Adaptador `SshTransport` con russh: conectar, auth (llave/contraseña), PTY, resize, keepalive | Integración contra el contenedor: `echo ok` devuelve `ok` |
| T-14 | Caso de uso `ConnectSession` (orquesta verificador, secretos, transporte) | Unit con mocks: rutas felices y cada error |
| T-15 | Comandos Tauri + channel binario para el stream de la terminal (entrada/salida/resize/cierre) | Test de integración del puente; sin copiar la salida al estado de React |
| T-16 | Componente `TerminalView` (xterm.js, fit, WebGL con fallback, copy/paste; atajos `Cmd` en macOS y `Ctrl+Shift` en Linux/Windows) | Vitest; vim y htop se ven correctamente en los 3 SO (prueba manual) |
| T-17 | Pestañas de sesión + estados (conectando, conectado, caído, reconectar) | Tests UI de transiciones de estado |

### F4 — Persistencia y gestor de sesiones (semana 7)
| ID | Ticket | Criterio clave |
|---|---|---|
| T-18 | `SessionRepository` en SQLite + migraciones | Tests CRUD contra SQLite en memoria |
| T-18b | Casos de uso `CreateSession`, `UpdateSession`, `DuplicateSession`, `DeleteSession` con gestión de credenciales: cambiar host/puerto/usuario/método de auth/llave; guardar, cambiar u olvidar contraseña y passphrase de llave en `SecretStore` (opción "recordar") | Unit con mocks: editar puerto/usuario persiste; nuevo secreto reemplaza al anterior; "olvidar" lo borra; cambiar de contraseña a llave borra la contraseña guardada; duplicar **no** copia secretos; borrar sesión borra sus secretos; los cambios aplican en la próxima conexión (la sesión abierta no se altera) |
| T-19 | UI: árbol de sesiones con carpetas, crear/editar/duplicar/borrar, búsqueda. Formulario de edición: host, puerto, usuario, método de auth, llave, contraseña/passphrase (campo vacío = no cambiar) + "recordar" + "olvidar credencial guardada" | Tests UI; validación del formulario reutiliza las reglas del dominio; nunca muestra ni prellena un secreto guardado (solo indica "guardado") |
| T-20 | Diálogos: passphrase, aceptar huella de host, error de auth. En error de auth con secreto guardado: pedir de nuevo y ofrecer actualizar el guardado | Tests UI; nunca se prellena el passphrase; reintento con secreto nuevo + "recordar" lo actualiza vía `UpdateSession` |

### F5 — SFTP sidebar (semanas 8–10) — el diferenciador
| ID | Ticket | Criterio clave |
|---|---|---|
| T-21 | Adaptador `RemoteFileSystem` con russh-sftp **sobre la misma conexión** que la terminal | Integración: list/stat/mkdir/rename/remove |
| T-22 | Transferencias por streaming con progreso, cancelación y cola | Archivo de 1 GB sin cargarlo en RAM; la cancelación limpia el parcial |
| T-23 | UI `SftpSidebar`: lista con iconos, ordenar, ruta editable, subir de nivel, refrescar | Tests UI |
| T-24 | Drag & drop: del SO al sidebar (subir) y del sidebar al SO (descargar) | Prueba e2e/manual en Linux, Windows y macOS (Finder) |
| T-25 | Layout redimensionable sidebar \| terminal (como MobaXterm), recordado por sesión | Tests UI del split |
| T-26 | Menú contextual: descargar, renombrar, borrar (con confirmación), nueva carpeta, copiar ruta, permisos (chmod en Linux) | Tests UI + integración |

### F6 — "Follow terminal folder" (semana 11)
| ID | Ticket | Criterio clave |
|---|---|---|
| T-27 | `CwdTracker` por **OSC 7**: parsear la secuencia en el stream sin alterar la salida | Unit sobre bytes de ejemplo, incluso secuencias partidas entre lecturas |
| T-28 | Inyección opcional de shell integration al abrir la sesión: bash/zsh (`PROMPT_COMMAND`/`precmd`) y PowerShell (`prompt`) | Integración: `cd /tmp` → el sidebar muestra `/tmp` |
| T-29 | Fallback: botón "sincronizar" que consulta `pwd` por un canal exec aparte | Funciona aunque la shell no emita OSC 7 |
| T-30 | Toggle "Follow" en la UI + manejo de rutas de Windows (`C:\` ↔ `/C:/`) | Tests unitarios de normalización de rutas |

### F7 — Calidad, empaquetado y release (semanas 12–13)
| ID | Ticket | Criterio clave |
|---|---|---|
| T-31 | Suite e2e con WebdriverIO + tauri-driver: conectar → `cd` → el sidebar sigue → subir/descargar | En verde en CI para Linux y Windows; en macOS, smoke test de arranque en CI + checklist manual |
| T-32 | Benchmarks de KPIs (sección 7) automatizados, con resultado en el resumen del job | Fallar el job si hay una regresión de más del 20 % |
| T-33 | Empaquetado: `.msi`/NSIS (Windows), AppImage + `.deb` (Linux), `.dmg` universal aarch64 + x86_64 (macOS), con checksums | Artefactos de los 3 SO publicados por un workflow con tag |
| T-33b | Firma y notarización en macOS (Developer ID + `notarytool` en CI). Si no hay cuenta de Apple Developer: `.dmg` sin firmar + instrucciones de Gatekeeper en el README | El `.dmg` abre en un Mac limpio (firmado) o el README documenta la apertura manual (sin firmar) |
| T-34 | Logs con `tracing` + redacción de secretos + "exportar diagnóstico" | Test: ningún secreto en logs |
| T-35 | README para portafolio: GIF de demo, diagrama de arquitectura, decisiones (ADRs), KPIs medidos | Revisión humana |
| T-36 | Prueba manual desde los 3 clientes (Linux, Windows, macOS) contra servidores reales (EC2 Linux y EC2 Windows con OpenSSH) | Checklist firmado en `docs/qa-v1.md` |

---

## 6. Definition of Done (por ticket)
- [ ] Test escrito antes que la implementación (se ve en el historial de commits: `test:` → `feat:`).
- [ ] `just check` en verde en local y en CI (Linux + Windows + macOS).
- [ ] Revisión de Claude Code: `APPROVE`.
- [ ] Sin `unwrap`/`expect` en código de producción; sin secretos en logs.
- [ ] Docs/ADR actualizados si cambió una decisión.
- [ ] Commit con Conventional Commits.

---

## 7. KPIs

### Producto (medidos en T-32)
| KPI | Meta v1 |
|---|---|
| Tiempo de conexión (LAN, llave Ed25519) | < 1,5 s |
| Latencia de tecla a eco (LAN) | < 16 ms p95 |
| RAM en reposo con 1 sesión abierta | < 100 MB |
| Throughput SFTP vs `sftp` de OpenSSH | ≥ 80 % |
| Tamaño del instalador | < 15 MB (< 25 MB el `.dmg` universal) |
| Arranque en frío | < 1 s |

### Ingeniería
| KPI | Meta |
|---|---|
| Cobertura `core` + `application` | ≥ 85 % |
| Cobertura `adapters` (integración) | ≥ 70 % |
| Vulnerabilidades altas/críticas (`cargo audit`, `npm audit`) | 0 |
| Tickets aprobados en la 1.ª revisión | ≥ 70 % |
| Rondas de revisión promedio por ticket | ≤ 1,5 |
| Tokens promedio por ticket (generador + revisor) | Medir en F0–F1 y fijar una meta de −20 % desde F3 |

---

## 8. Riesgos y mitigaciones
| Riesgo | Mitigación |
|---|---|
| Algún formato PPK (Argon2, v3) no es compatible con russh | Spike T-06 antes de comprometer la arquitectura; plan B con `ssh-key` (`ppk`) |
| La shell remota no emite OSC 7 (servidores restringidos, cmd.exe) | Inyección opcional (T-28) + botón de sincronizar (T-29) |
| Renderizado WebGL inestable en algunas GPU/VM | Fallback automático a canvas/DOM |
| Diferencias de keyring en Linux (sin Secret Service en algunos WM) | Detectarlo y ofrecer un almacén cifrado con contraseña maestra (Argon2id + XChaCha20-Poly1305) como fallback |
| El generador se sale del alcance y gasta tokens | Tickets con la lista de archivos cerrada + el revisor rechaza cambios fuera de alcance |
| Diferencias entre WebViews (WKWebView/WebKitGTK/WebView2) en WebGL, fuentes y drag & drop | CI en los 3 SO + checklist visual por plataforma en T-36 |
| Sin WebDriver para macOS (no hay e2e completo) | Lógica cubierta por tests unit/integración (independientes del SO) + smoke test + QA manual |
| Firma/notarización de Apple cuesta 99 USD/año | Opcional para v1 (T-33b); se distribuye sin firmar con instrucciones |
| Runners macOS de GitHub Actions más lentos y con más consumo de minutos | Ejecutar macOS solo en PR a `main` y en releases; Linux/Windows en cada push |
| Bloqueo del runtime con transferencias grandes | Streaming por chunks + tareas Tokio separadas por transferencia |

---

## 9. Backlog post-v1 (orden sugerido)
1. Túneles (local/remote/dynamic) con UI tipo MobaXterm.
2. Terminal local (ConPTY / PTY) en pestañas.
3. Editor remoto rápido (abrir → editar → guardar por SFTP).
4. Multiexec (mismo comando en varias sesiones).
5. Jump host / ProxyJump.
6. Soporte de ssh-agent y Pageant.
7. Importar sesiones desde MobaXterm (`.mxtsessions`) y PuTTY.
