# F8: túneles SSH (local, remoto y dinámico)

**Objetivo (humano, 2026-11-27):** F7 → F8 → **beta 1 funcional** (`v0.1.0-beta.1`). D4 se toma con su valor por defecto: se muestra "Puerto local en uso".

Primer ítem del backlog post-v1 (plan, sección 9). Diseño en **ADR-007** (Propuesto).
Sin dependencias nuevas: `russh 0.65.4` ya trae `channel_open_direct_tcpip`, `tcpip_forward`, `cancel_tcpip_forward`, `Handler::server_channel_open_forwarded_tcpip` y `Channel::into_stream`.

## Principio (requisito del humano, 2026-11-27)
**El túnel lo resuelve 100 % la app.** El usuario elige una sesión guardada, el tipo y los puertos, y pulsa un interruptor. La app se encarga del resto:
- abre su propia conexión SSH en segundo plano (sin pestaña de terminal), reutilizando las credenciales de la sesión;
- la comparten todos los túneles de esa sesión;
- si se cae, reconecta sola con backoff (1, 2, 4… hasta 30 s);
- cierra la conexión al detener el último túnel.

El usuario **solo ve tres errores**: `No se pudo conectar al servidor`, `Llave inválida` y `Contraseña incorrecta`. El detalle técnico va al log (T-34a) y al diagnóstico (T-34b).

Hay dos diálogos que no son errores y se mantienen, porque ya existen y protegen al usuario:
- confirmar la huella de un host nuevo o cambiado (ADR-005, no se debilita);
- pedir la passphrase o la contraseña si la sesión no la recuerda (decisión de F4).

## Tickets

| Ticket | Alcance | Capa | Depende de |
|---|---|---|---|
| T-40a | Dominio `TunnelSpec` (`Local`, `Remote`, `Dynamic`) con validación (puertos 1–65535, bind en loopback por defecto, `allow_lan` explícito), `TunnelError` de cara al usuario (`Connection`, `InvalidKey`, `InvalidPassword`) + puerto `PortForwarder` | core | F7 |
| T-40b | `PortForwarder` para `RusshTransport`: `direct-tcpip` → stream. Integración: túnel a `127.0.0.1:22` del contenedor que lee el banner `SSH-2.0` | adapters | T-40a |
| T-40c | `LocalForwarder` (`TcpListener` + `copy_bidirectional` por conexión, parada limpia, contadores) + casos de uso `StartTunnel`/`StopTunnel`/`ListTunnels` | adapters + application | T-40b |
| T-40d | `TunnelConnections`: conexión SSH en segundo plano por sesión (reutiliza el caso de uso de conexión y la autenticación existentes), compartida por sus túneles, con reconexión con backoff y cierre con el último túnel. Mapeo de cualquier error a los 3 de `TunnelError`. Integración: matar la conexión → reconecta y el túnel vuelve a funcionar | application + adapters | T-40c |
| T-40e | Túnel remoto: `tcpip-forward` + enrutado de `forwarded-tcpip` desde el `Handler`, rechazando lo no registrado. Integración: `nc` dentro del contenedor → eco local | adapters | T-40d |
| T-40f | Túnel dinámico: SOCKS5 mínimo (sin auth, solo CONNECT, IPv4/IPv6/dominio). Parser puro + integración | adapters | T-40d |
| T-40g | Persistencia: tabla `tunnels` (SQLite v3) ligada a la sesión, con `auto_start` | core + adapters | T-40a |
| T-40h | Desktop: comandos `tunnel_*` + evento `tunnels` (estado y bytes, con throttle de 500 ms) + `auto_start` al abrir la app. Al evento solo llegan los 3 errores | desktop | T-40e, T-40f, T-40g |
| T-40i | UI de un clic: vista "Túneles" con una lista. Cada fila tiene el nombre, la sesión, `local:puerto → destino`, un interruptor ON/OFF y un punto de estado (gris, verde, ámbar al reconectar, rojo con el mensaje de error). El formulario es corto: sesión (desplegable), tipo, puerto local y destino; los valores por defecto se rellenan solos | ui | T-40h |

## Regla de logs (humano, 2026-11-27; obligatoria desde F8)
Todo error capturado se registra; **ningún catch sin log**:
- **Rust:**
  - `map_err(|_| ..)`, `let _ = ..`, `.ok()`, `unwrap_or*` sobre `Result` y `if let Err`/`match Err` llevan `tracing::warn!`/`error!` con el contexto (operación, ids, variante);
  - cada operación relevante tiene su `info!` de inicio o fin.
- **UI:** cada `catch` registra el error a través de `api.ts` (`logUiError`, que llega al log del backend) antes de mostrarlo.
- **Rutas calientes** (bytes del PTY, bloques de transferencias): se loguea el primer fallo y luego se resume, sin un log por bloque.
- Nunca secretos ni contenido (T-34a).

Cada ticket de F8 incluye esta regla en sus criterios.

## Después de F8 (pedido del humano)
1. Indexar el proyecto con `codegraph init` (añadir `.codegraph/` a `.gitignore`).
2. Una sesión de Claude **nueva y sin contexto** revisa el código contra los requisitos (plan v1, AGENTS.md, CLAUDE.md, ADRs, tickets) y corrige ella misma lo que haga falta, en su propia rama, con `just check` en verde.
3. El orquestador revisa esos cambios y luego audita la regla de logs en **todo** el código (F0–F8), no solo en F8. Los huecos se corrigen con tickets T-5x.

## Reglas de seguridad (ADR-007)
- Los túneles locales y dinámicos escuchan en `127.0.0.1` por defecto. Para escuchar en la LAN hay que marcar `allow_lan`, con aviso.
- El túnel remoto pide `localhost` en el servidor por defecto.
- Se rechaza cualquier `forwarded-tcpip` no registrado.
- En los logs: solo id, tipo, puertos, resultado y bytes. Nunca el contenido ni secretos.
- La verificación de la llave del host no cambia: un host nuevo o con huella distinta pasa por el diálogo de siempre.

## Decisiones (valores por defecto; no bloquean)
- **D1:** alcance de F8 = solo túneles.
- **D2:** `allow_lan` permitido, como opción explícita con aviso.
- **D3:** `auto_start` por túnel, desactivado por defecto.
- **D4:** si el puerto local está ocupado, la app muestra *"Puerto local en uso"*. Es la única excepción propuesta a los 3 errores; sin ella, el túnel fallaría sin explicación.
- **ADR-007:** aprobar el borrador.
