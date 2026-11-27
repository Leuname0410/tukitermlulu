# ADR-005: Handshake verificado y plano de datos de la terminal en el puerto SSH

**Estado:** Propuesto (pendiente de aprobación humana) · **Fecha:** 2026-11-26

## Contexto
El puerto `SshTransport` de T-08a no permitía al caso de uso verificar la llave del host antes de enviar credenciales, ni transportar los bytes del PTY. `core` no puede depender de tokio, y la salida de la terminal nunca debe pasar por el estado de React (plan §3).

## Decisión
- `connect` devuelve `Connected { id, fingerprint }`: el handshake termina, pero la conexión **no se autentica** hasta que el caso de uso (T-14) haya verificado la huella con `HostKeyVerifier`. Si es `Unknown`/`Changed`, se cierra sin enviar credenciales.
- Plano de datos: `open_pty(conn, size, Box<dyn PtySink>)` + `write(channel, &[u8])`. `PtySink` es un trait **síncrono** (`data`, `closed`) que el adaptador invoca desde su tarea Tokio; la composition root (T-15) lo implementa reenviando a un Tauri channel binario.
- `dyn` solo en `PtySink` (síncrono, dyn-compatible); los puertos async siguen la opción B (genéricos + `impl Future`).

## Alternativas descartadas
- **Stream async (`impl Stream`) en el puerto:** requiere `futures`/tokio en `core` o tipos complejos de mockear.
- **Canal mpsc de tokio en la firma:** acopla `core` a tokio.
- **Verificar la huella dentro del adaptador:** mezcla la política (TOFU, UI de aceptación) con el transporte.

## Consecuencias
- (+) `core` sigue sin runtime; el caso de uso controla la confianza; la terminal no toca React.
- (−) El `PtySink` debe ser barato y no bloquear (se documenta en el trait); el backpressure queda en manos del channel de Tauri.
