# F7: calidad, empaquetado y release (plan de tickets)

Orden: primero lo que no depende de decisiones externas.

| Ticket | Alcance | Depende de | Decisión |
|---|---|---|---|
| T-34a | Logs con `tracing` en `desktop` y `adapters`: archivo rotativo en `app_log_dir`, nivel por `TUKI_LOG`, capa que redacta campos sensibles (`password`, `passphrase`, `secret`, `key`, `token`) y test de que ningún secreto llega al log | T-30 | no |
| T-34b | "Exportar diagnóstico": un `.txt` en Descargas con la versión, el SO y los últimos logs (ya redactados), más un botón en la UI (sin `zip`, sin dependencias nuevas) | T-34a | no |
| T-33a | `just bundle` + configuración del bundle de Tauri: `.msi`/NSIS, AppImage + `.deb` y `.dmg` universal; metadatos (descripción, licencia MPL-2.0, categoría) | T-34b | no |
| T-33c | Workflow `release.yml` al hacer push de un tag `v*`: matriz de 3 SO, `tauri-action` o `tauri build`, checksums SHA-256 y release en borrador con los artefactos (`permissions: contents: write` solo en ese workflow) | T-33a | **D2** |
| T-33b | macOS: firma + `notarytool` si hay Developer ID; si no, `.dmg` sin firmar + instrucciones de Gatekeeper en el README | T-33c | **D1** |
| T-32a | Benchmarks de los KPIs medibles sin GUI: conexión, latencia tecla→eco, throughput SFTP frente a `sftp` (contenedor), tamaño del instalador. Resultado en `$GITHUB_STEP_SUMMARY`; falla si empeora más del 20 % respecto a `docs/kpi-baseline.json` | T-33c | **D4** |
| T-32b | Cobertura con `cargo-llvm-cov` (metas `core`+`application` ≥ 85 % y `adapters` ≥ 70 %) en CI | T-32a | **D4** |
| T-31a | e2e con WebdriverIO + `tauri-driver` en Linux (CI con el contenedor): conectar → `cd /tmp` → el sidebar sigue → subir → descargar. También mide el arranque en frío y la RAM en reposo (KPIs con GUI) | T-32b | **D3** |
| T-31b | e2e en Windows (CI) | T-31a | **D3** |
| T-31c | Smoke de arranque en macOS (CI) + checklist manual | T-31b | no |
| T-35 | README de portafolio: diagrama de arquitectura (mermaid), ADRs, KPIs medidos. El GIF de la demo lo graba el humano | T-31c | (GIF humano) |
| T-36 | Checklist manual en `docs/qa-v1.md` desde los 3 clientes contra EC2 Linux y EC2 Windows | todo | humano |

## Decisiones para el humano
- **D1 (T-33b):** ¿Tienes cuenta de Apple Developer (99 USD/año)?
  - **Sí:** firma y notarización en CI. Necesito que cargues los secretos `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_ID`, `APPLE_TEAM_ID` y `APPLE_PASSWORD` en GitHub.
  - **No:** `.dmg` sin firmar más instrucciones de Gatekeeper.
- **D2 (T-33c):** Windows sin firma de código. SmartScreen avisará la primera vez; el plan no incluye certificado. ¿Aceptas publicar sin firmar en v1? (recomendado).
- **D3 (T-31b):** El runner de Windows de GitHub no ejecuta contenedores Linux.
  - Propuesta: en Windows, el e2e se conecta al **OpenSSH Server del propio runner** (`Add-WindowsCapability`). De paso prueba un servidor Windows real: PowerShell 5.1/cmd, rutas `C:\` y la integración de shell.
  - Alternativa: solo Linux en CI y Windows a mano.
- **D4 (T-32):** Los runners de CI son ruidosos.
  - Propuesta: la línea base se fija con la mediana de 5 ejecuciones y se guarda en `docs/kpi-baseline.json` (la actualizo con un ticket cuando cambie a propósito). La regla del 20 % se aplica sobre la mediana de 5 repeticiones.
  - El job de benchmarks y el de cobertura corren solo en PR a `main` y en tags, no en cada push.
