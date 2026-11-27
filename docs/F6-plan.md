# F6 — Follow terminal folder: plan de tickets (borrador)

Orden: T-27 → T-28a → T-28b → T-29 → T-30. T-27 no depende de la UI y puede ir en paralelo con F5; T-28b y T-30 necesitan el `SftpSidebar` (T-23).

| Ticket | Alcance | Archivos (≤5) | Riesgo |
|---|---|---|---|
| T-27 | `Osc7Tracker` (adapters): parser puro por estados, fragmentos partidos, tope 4096, `%XX` | `adapters/src/cwd_tracker.rs`, `lib.rs` | bajo |
| T-28a | Cablear el tracker en `ChannelSink` (desktop): por cada `data`, `feed` y, si cambia, enviar `{ cwd }` por un tercer `Channel` (`on_cwd`); `api.ts` expone `onCwd` en `TerminalHandlers` | `desktop/terminal.rs`, `ui/src/lib/api.ts`, `api.test.ts`, `tabController.ts`, `tabs.test.ts` | bajo |
| T-28b | Inyección **opcional** de shell integration tras `open_pty`: snippet POSIX para bash/zsh (`PROMPT_COMMAND` / `precmd`) y otro para PowerShell (`prompt`); flag por sesión `shell_integration` (migración v2 en SQLite) y casilla en `SessionForm` | `application` (caso de uso o paso en `ConnectSession`), `session_repository.rs` (migración v2), `sessions.rs`, `SessionForm.tsx`, fixtures | **medio**: ver D1/D2 |
| T-29 | Botón "Sincronizar" en el sidebar | ver D3 | **ticket mal planteado** |
| T-30 | Toggle "Follow" por pestaña en el sidebar + normalización de rutas Windows (`C:\x` ↔ `/C:/x`, `file:///C:/x`) como función pura en `core::remote_path` | `core/src/remote_path.rs`, `SftpSidebar`, tests | bajo |

## Decisiones para el humano

**D1 — Cómo se inyecta (T-28b).** Escribir el snippet en el PTY lo hace visible en la terminal (eco). Propuesta: enviarlo con espacio inicial (no entra en el historial con `HISTCONTROL=ignorespace`), seguido de `clear` opcional. Alternativa sin eco: `SendEnv`/`setenv` + un rc remoto → exige permisos en el servidor (`AcceptEnv`), poco portable. **Recomiendo: inyección por PTY, desactivada por defecto, casilla por sesión.**

**D2 — Detección de shell.** El snippet POSIX y el de PowerShell son incompatibles. Propuesta: antes de inyectar, un canal `exec` aparte con `echo $0; echo $SHELL` (en PowerShell devuelve literal) decide cuál enviar. Si no se reconoce, no se inyecta. Requiere añadir `bash` al contenedor de tests (Alpine usa `ash`, que no tiene `PROMPT_COMMAND`) → modificar `tests/fixtures`.
→ Merece **ADR-006: Shell integration por inyección en el PTY** (te paso el borrador si apruebas D1/D2).

**D3 — T-29 no funciona tal como está escrito.** Un canal `exec` aparte abre **otra** shell que arranca en `$HOME`: su `pwd` no es el directorio de la shell interactiva. Alternativas:
- (a) Al pulsar "Sincronizar", escribir en el PTY (con espacio inicial) `printf '\033]7;file://%s%s\007' "$HOSTNAME" "$PWD"`: reutiliza T-27, funciona en cualquier shell POSIX sin integración; visible en la terminal. **Recomendada.**
- (b) `exec` con `readlink /proc/<pid>/cwd` del proceso de la shell: invisible, pero solo Linux y hay que averiguar el PID del PTY (no es trivial ni fiable).
- (c) Quitar T-29 y depender de T-28b.

**D4 — Prioridad.** ¿F6 después de cerrar F5 (orden del plan) o T-27 ya en paralelo mientras F5 avanza?
