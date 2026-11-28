# ADR-007: Túneles SSH

**Estado:** Propuesto
**Fecha:** 2026-11-27

## Contexto
El primer ítem del backlog post-v1 son los túneles local, remoto y dinámico, al estilo de MobaXterm. Comparten la conexión SSH ya autenticada de la pestaña.

## Decisión
1. **Puerto en `core`:** `PortForwarder`, con tres operaciones:
   - `open_direct(conn, host, port, origin) -> stream`;
   - `request_remote(conn, bind, port) -> bound_port`;
   - `cancel_remote(conn, bind, port)`.

   El stream es un tipo asociado `AsyncRead + AsyncWrite` definido en adapters. `core` solo ve el trait, sin tokio, mediante un tipo opaco.
2. **Listeners en adapters:** `LocalForwarder` y `SocksForwarder` abren `tokio::net::TcpListener`. Cada conexión aceptada abre un canal `direct-tcpip` y copia en ambos sentidos con `tokio::io::copy_bidirectional`. Una tarea por conexión, cancelable con un token por túnel.
3. **Túnel remoto:** el `Handler` de russh recibe `forwarded-tcpip` y busca en un registro `(bind, port) → destino local`. Si no lo encuentra, rechaza el canal.
4. **SOCKS5 propio y mínimo:** sin autenticación, solo `CONNECT`, con direcciones IPv4, IPv6 y de dominio. El dominio se resuelve en el servidor, porque se envía tal cual en `direct-tcpip`. Son unas 100 líneas: no se añade ninguna dependencia.
5. **Bind:** loopback por defecto. Cualquier otro bind exige `allow_lan`, una marca explícita guardada en el túnel.
6. **Conexión gestionada por la app:**
   - Cada sesión con túneles activos tiene una conexión SSH propia en segundo plano, independiente de las pestañas de terminal, que comparten todos sus túneles.
   - Si se cae, reconecta con backoff exponencial (1 s → 30 s) y se cierra al detener el último túnel.
   - El usuario solo ve `Connection`, `InvalidKey` e `InvalidPassword`, más "puerto local en uso". El resto va al log.
7. **Persistencia:** tabla `tunnels` (migración v3) ligada a la sesión, con borrado en cascada.

## Consecuencias
- Sin dependencias nuevas.
- Un túnel nunca expone puertos a la red sin que el usuario lo marque explícitamente.
- Cerrar la conexión cierra sus túneles. Reconectar no los relanza salvo que estén marcados con `auto_start`.
- Queda fuera: X11 forwarding, ProxyJump (ítem 5 del backlog) y la autenticación SOCKS.
