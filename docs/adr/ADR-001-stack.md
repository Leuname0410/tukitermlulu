# ADR-001: Stack tecnológico

**Estado:** Propuesto (pendiente de aprobación humana) · **Fecha:** 2026-11-25

## Contexto
Cliente SSH/SFTP de escritorio, multiplataforma (Linux, Windows, macOS), proyecto de portafolio. Necesita binario pequeño, arranque rápido (< 1 s), bajo consumo en reposo (< 100 MB) y una terminal de alto rendimiento.

## Decisión
- **Shell:** Tauri 2 (WebView del sistema: WebView2 / WebKitGTK / WKWebView).
- **Backend:** Rust stable (MSRV = estable actual, hoy 1.100), Tokio, `thiserror`, `tracing`, `serde`.
- **SSH/SFTP:** `russh` + `russh-sftp` (Rust puro, async). Soporte PPK pendiente del spike T-06 (plan B: `ssh-key` con feature `ppk`).
- **Persistencia:** SQLite con migraciones versionadas. **Secretos:** `keyring` + `zeroize`.
- **UI:** React + TypeScript strict + Vite, Zustand, Tailwind; terminal xterm.js (+ fit, + webgl con fallback).
- **Calidad:** `just check` = fmt, clippy `-D warnings`, tests, `cargo audit`, `cargo deny`, ESLint (typescript-eslint strict), typecheck, Vitest, `npm audit`. CI en matriz de 3 SO.
- **Licencia:** MPL-2.0.

## Alternativas descartadas
- **Electron:** instalador ≥ 80 MB y RAM en reposo > 150 MB; incumple los KPIs.
- **libssh2 / ssh2-rs:** dependencia C, API bloqueante; complica Windows y async.
- **UI nativa (egui/iced/Qt):** sin un emulador de terminal maduro equivalente a xterm.js.

## Consecuencias
- (+) Binarios pequeños, un solo lenguaje en el backend, seguridad de memoria.
- (−) Tres WebViews distintas: QA visual por plataforma (T-36). Sin WebDriver para WKWebView.
- (−) Dependencias transitivas de Tauri en Linux (gtk-rs 0.18) con avisos `unsound`/`unmaintained` sin parche: se vigilan en `cargo audit`, no se ignoran.
- Notas del scaffold: paquetes con prefijo `tuki-` (Cargo no admite un paquete `core`); TypeScript fijado a 5.9 por compatibilidad con typescript-eslint.
