# F6 — Follow terminal folder: plan de tickets (v2, tras D1–D4)

Decisiones del humano:
- **D1:** al abrir la sesión arrancan la terminal y el sidebar SFTP a la izquierda, automáticamente.
- **D2:** la solución tiene que servir a largo plazo con servidores Win, Linux y Mac.
- **D3:** el sidebar sigue el directorio de la terminal, con una casilla para dejar de seguirlo.
- **D4:** analizar si es viable.

Diseño en **ADR-006** (propuesto).

## Viabilidad (D4): **viable**
- **OSC 7 + inyección por shell** cubre bash/zsh (Linux, macOS desde Catalina), fish, PowerShell y cmd.exe (OpenSSH de Windows) sin configurar el servidor.
- **Parte con más riesgo:** ocultar el eco de la inyección. Se resuelve con un marcador y un timeout de 2 s; peor caso: se ve una línea un instante.
- **Limitaciones aceptables:**
  - Shells exóticas (`ash`/`sh` de busybox, `csh`) o cuentas restringidas: sin seguimiento. El sidebar funciona igual con navegación manual.
  - `su`/`exec bash` pierden el hook hasta reconectar.
- **T-29** ("sincronizar" por `exec`) se **descarta**: un canal `exec` no ve el directorio de la shell interactiva. Lo sustituye la detección más la inyección.
- **D1 afecta a F5:** T-23/T-25 deben abrir el sidebar al conectar, a la izquierda, empezando en el directorio inicial de SFTP (normalmente `$HOME`).

## Tickets
| Ticket | Alcance | Depende |
|---|---|---|
| T-27 | `Osc7Tracker` (parser puro, fragmentos partidos, tope 4096) | T-21 |
| T-28a | Cablear el tracker en `ChannelSink`: canal `on_cwd` y `onCwd` en `api.ts`/`TabController`, con el cwd en el estado de la pestaña | T-27 |
| T-28b | `ShellProbe` por canal `exec` (detección de shell) + líneas de inyección por shell (función pura en `application`, testeable con mocks) | T-28a |
| T-28c | Inyección tras ~300 ms de silencio + ocultación de salida hasta el marcador (con timeout de 2 s) en el adaptador/sink; opción por sesión `shell_integration` (migración SQLite v2, casilla en `SessionForm`); `bash`/`zsh`/`fish` en el contenedor; integración: `cd /tmp` → `on_cwd` = `/tmp` sin eco visible | T-28b |
| T-30 | Casilla "Seguir terminal" (por defecto activa) en `SftpSidebar` + normalización de rutas Windows (`C:\x`, `/C:/x`, `file:///C:/x`) en `core::remote_path` | T-23, T-28a |

## Orden propuesto
T-27 va ya, en paralelo con F5: no toca la UI.
T-28a–c van tras T-23, que necesita el sidebar visible para probar el seguimiento.
T-30 cierra F6.
