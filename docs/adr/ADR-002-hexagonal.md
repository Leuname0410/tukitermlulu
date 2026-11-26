# ADR-002: Arquitectura hexagonal (puertos y adaptadores)

**Estado:** Propuesto (pendiente de aprobación humana) · **Fecha:** 2026-11-25

## Contexto
El núcleo (sesiones, auth, follow-cwd) debe probarse sin red, disco ni GUI, y los protocolos (SFTP hoy; túneles, terminal local, RDP mañana) deben añadirse sin tocar el dominio. Además el código lo genera un agente: las fronteras deben ser verificables en revisión.

## Decisión
Workspace de Cargo con cuatro crates y dependencia en un solo sentido:

```
tuki-desktop → tuki-adapters → tuki-application → tuki-core
```

- **core:** entidades, value objects validados, errores de dominio y **puertos** (traits): `SshTransport`, `RemoteFileSystem`, `KeyLoader`, `SecretStore`, `HostKeyVerifier`, `SessionRepository`, `CwdTracker`. Sin I/O, sin tokio, sin tauri.
- **application:** un caso de uso por struct con `execute`; recibe los puertos por constructor (DIP). Solo depende de `core`.
- **adapters:** implementaciones (russh, russh-sftp, keyring, SQLite, known_hosts). Tests de integración contra el contenedor OpenSSH de `tests/fixtures`.
- **desktop:** composition root de Tauri. Plano de control por `invoke`; plano de datos (bytes de terminal) por **Tauri channels**, nunca por estado de React.

## Alternativas descartadas
- **Un solo crate con módulos:** las capas no las hace cumplir el compilador; un `use` cruzado pasa revisión sin que nadie lo note.
- **Capas por feature (vertical slices):** útil en apps CRUD; aquí los adaptadores (una conexión SSH compartida por terminal y SFTP) son transversales.

## Consecuencias
- (+) Casos de uso testeables con `mockall`; el revisor comprueba capas mirando los `Cargo.toml`.
- (+) Nuevos protocolos = nuevos adaptadores (OCP).
- (−) Más boilerplate (traits + mocks) y más crates que compilar.
- Pendiente: `AGENTS.md` lista además un puerto `ServerMetricsProvider` que no está en el plan; decidir si se añade (nuevo ADR) o se quita de `AGENTS.md`.
