# AGENTS.md — Generador (OpenCode)

Cliente SSH/SFTP de escritorio, open source, inspirado en MobaXterm.
Tu rol: **implementar UN ticket a la vez con TDD**. No diseñas arquitectura ni amplías el alcance.

## Reglas de oro
1. Lee el ticket completo. Toca **solo** los archivos que el ticket lista. Si necesitas otro, detente y repórtalo.
2. **TDD estricto**: test rojo → implementación mínima → refactor. Haz commits separados: `test:` y luego `feat:`/`fix:`.
3. No explores el repo "por si acaso". Las firmas e interfaces vienen en el ticket.
4. Antes de entregar: `just check` debe quedar en verde. Si falla, corrígelo tú.
5. Diff máximo ~300 líneas. Si no cabe, detente y propón cómo partir el ticket.
6. No cambies dependencias, CI, `justfile` ni archivos `docs/adr/*` salvo que el ticket lo pida.

## Stack (fijo)
- **Rust stable**, Tokio, `thiserror`, `tracing`, `serde`
- SSH/SFTP: `russh`, `russh-sftp` · Secretos: `keyring`, `zeroize` · DB: SQLite
- **Tauri 2** (crate `desktop`)
- UI: React + TypeScript (strict) + Vite, Zustand, Tailwind, xterm.js
- Tests: `cargo test`, `mockall`, `testcontainers`; Vitest + Testing Library; WebdriverIO + `tauri-driver`

## Estructura y dependencias entre capas
```
crates/core/         Dominio puro. Entidades, value objects, traits (puertos), errores. SIN I/O, SIN tokio, SIN tauri.
crates/application/  Casos de uso. Depende SOLO de `core`. Recibe los puertos por constructor (genéricos o Arc<dyn Trait>).
crates/adapters/     Implementaciones de los puertos (russh, sftp, keyring, sqlite, known_hosts).
crates/desktop/      Tauri: comandos, channels y composition root (aquí se cablea todo).
ui/                  React. Habla con el backend solo vía ui/src/lib/api.ts (adaptadores tipados).
tests/fixtures/      Llaves y docker-compose de prueba. NO regenerar.
```
Dirección permitida: `desktop → adapters → application → core`. Nunca al revés.

## Puertos existentes (en `core`)
`SshTransport`, `RemoteFileSystem`, `KeyLoader`, `SecretStore`, `HostKeyVerifier`, `SessionRepository`, `CwdTracker`, `ServerMetricsProvider`.
Si un ticket necesita un puerto nuevo, lo definirá el ticket.

## Convenciones Rust
- Prohibido en producción: `unwrap`, `expect`, `panic!`, `todo!`, `unimplemented!`, indexado `v[i]` sin chequear. En tests sí se permiten.
- Errores: un `enum` con `thiserror` por crate. Propaga con `?`. Mensajes accionables para el usuario.
- Un caso de uso = un struct con un método `execute` (SRP).
- Async: nada de I/O bloqueante en contexto async; usa `spawn_blocking` si es inevitable.
- Nombres de tipos, funciones y variables **en inglés**. Comentarios y docs en español.
- Value objects con constructor validado (`Port::new(u16) -> Result<Port, DomainError>`) en vez de primitivos sueltos.
- `pub(crate)` por defecto; `pub` solo para lo que se exporta.

## Seguridad (obligatorio)
- Passphrases, contraseñas y llaves privadas: envueltas en `Zeroizing<...>` o `SecretString`.
- Esos tipos **no** implementan `Debug`/`Display`/`Serialize` con su contenido (usa `[REDACTED]`).
- Nunca escribas secretos en SQLite, logs, errores ni eventos hacia la UI.
- No desactives la verificación de la llave del host, ni siquiera en tests de integración (usa el known_hosts del fixture).
- Algoritmos heredados (`ssh-rsa` SHA-1, `diffie-hellman-group1`) solo si el ticket lo pide, como opt-in.
- Permisos de archivos de llaves copiados: 0600 (Unix) / ACL solo del usuario (Windows).

## Logs (obligatorio)
- **Ningún error capturado sin log.** En Rust, todo `map_err(|_| ..)`, `let _ = <Result>`, `.ok()`, `unwrap_or*` sobre un `Result`, `if let Err` o `match .. Err` registra `tracing::warn!` (recuperable) o `tracing::error!` (falla la operación), con contexto: operación, ids y variante del error.
- Cada operación relevante (conectar, autenticar, abrir o cerrar un canal, cada operación SFTP, transferencia o túnel) emite `info!` al terminar, con su resultado.
- **UI:** todo `catch` registra el error con `logUiError` de `ui/src/lib/api.ts` antes de mostrarlo o descartarlo.
- **Rutas calientes** (bytes del PTY, bloques de transferencia): se loguea el primer fallo y un resumen al final, nunca un log por bloque.
- Prohibido loguear secretos (contraseñas, passphrases, llaves), bytes de la terminal o contenido de archivos. Las rutas remotas, solo en `debug!`.

## Terminal y rendimiento
- Los bytes de la terminal van por **Tauri channels**, nunca por `invoke` ni por estado de React/Zustand.
- Transferencias SFTP por streaming en chunks; nunca cargues el archivo completo en memoria.

## Convenciones TypeScript / UI
- `strict: true`; prohibido `any` (usa `unknown` y estrecha el tipo).
- Componentes funcionales; la lógica va en hooks y stores, no en el JSX.
- Toda llamada al backend pasa por `ui/src/lib/api.ts`.
- Tests con Testing Library por rol o texto visible, no por clases CSS.

## Tests
- Nombre: `should_<resultado>_when_<condición>` (Rust) / `it("should ... when ...")` (TS).
- Unit en `core`/`application` con mocks de `mockall`. Sin red ni disco.
- Integración en `adapters` contra el contenedor OpenSSH: `docker compose -f tests/fixtures/docker-compose.yml up -d`.
- Cada criterio de aceptación del ticket ⇒ al menos un test que lo cubra.

## Comandos
```
just dev          # app en modo desarrollo
just check        # fmt + clippy -D warnings + test + audit + deny + eslint + vitest (gate obligatorio)
just test-int     # tests de integración (requiere Docker)
just e2e          # end-to-end
```

## Commits
Conventional Commits, en inglés, con el ID del ticket: `feat(sftp): stream uploads with progress [T-22]`.

## Entrega
Termina con un resumen de máximo 10 líneas:
- Ticket y criterios cumplidos (uno por línea).
- Archivos tocados.
- Tests añadidos.
- Dudas o desviaciones (si no hay, escribe "ninguna").
No pegues el diff ni el código en el resumen.
