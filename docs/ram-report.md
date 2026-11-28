# RAM en reposo: investigación T-31d

## Método

- Windows 11, WebView2 158.0.4370.45, build release de Tauri, ventana 1200×800, una sesión SSH en el contenedor fixture.
- Cada experimento se midió con `just e2e` tres veces; la tabla muestra la **mediana**. La RAM es la suma de `PrivatePageCount` del proceso `tuki-desktop.exe` y todos sus descendientes, tomada 5 s después de conectar.
- El desglose agrupa por `--type=` de la línea de comandos; `browser` es el `msedgewebview2.exe` sin `--type=`. `otros` incluye `crashpad-handler` (~2,7 MB) y `conhost.exe` (~2,1 MB).
- Latencia: el e2e escribe 200 caracteres en la terminal y espera a que cambie la huella visual (captura del elemento, válida para WebGL y DOM); mediana de 3.
- Al fijar `additionalBrowserArgs` se **reemplaza** el valor completo que pasa wry. El default que hay que conservar es `--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection` (ver `wry/src/webview2/mod.rs`). Los argumentos `--autoplay-policy` y `--proxy-server` solo se añaden si la app habilita autoplay o configura proxy; aquí no aplican.

## Experimentos

| Experimento | MB totales | app / browser / renderer / gpu (MB) | Δ vs base | Latencia 200 chars | e2e funcional |
| --- | ---: | --- | ---: | ---: | --- |
| 1. Base | 284,6 | 10,9 / 36,0 / 77,1 / 134,8 | — | 254 ms | PASS (3/3) |
| 2a. `--disable-gpu` | 179,4 | 10,9 / 35,6 / 80,3 / 27,2 | −37,0 % | 241 ms | PASS (3/3) |
| 2b. Solo `--disable-features=…` (default explícito) | 285,4 | 11,9 / 35,6 / 77,2 / 134,9 | +0,3 % | 264 ms | PASS (3/3) |
| 2c. `--renderer-process-limit=1` | 289,4 | 11,1 / 36,7 / 82,5 / 134,7 | +1,7 % | 242 ms | PASS (3/3) |
| 3. Renderer DOM (sin addon WebGL) | 268,9 | 11,7 / 35,5 / 73,5 / 121,9 | −5,5 % | 247 ms | PASS (3/3, 1 reintento por flake de `tauri-driver`) |
| 4. Combinación de ganadores (= solo 2a) | 176,6 | 10,9 / 35,6 / 78,6 / 27,1 | −37,9 % | 254 ms | PASS (3/3) |

El resto de grupos se mantuvo estable: utility ~19,8 MB y otros ~4,8 MB en todos los experimentos.

## Decisión

Se adopta **2a** (`--disable-gpu` conservando el default de `--disable-features`): cumple el umbral de ≥10 %, el e2e sigue en verde y no toca xterm. El renderer DOM no se adopta: −5,5 % está por debajo del umbral, aunque la latencia no empeora (−2,8 %). 2b es equivalente a la base y 2c la empeora. No hay segunda combinación posible porque solo hay un ganador; la fila 4 verifica el estado final adoptado.

## Recomendación de meta

- **Windows (medido):** fijar la meta en **≤ 185 MB** en reposo con 1 sesión (mediana actual 176,6 MB; margen para ruido de medición). El objetivo original de < 100 MB no es realista con WebView2: la app Rust son solo ~11 MB y el resto es el motor (browser ~36 MB, renderer ~79 MB, gpu ~27 MB, utility + otros ~25 MB). Bajar de ~150 MB exigiría cambiar de motor o reducir el árbol de procesos, fuera de alcance (ADR).
- **Linux / macOS (estimación, sin medir):** WebKitGTK y WKWebView suelen compartir más memoria con el proceso de la app y no arrancan un gpu-process equivalente; estimación aproximada de **90–140 MB** en Linux y **100–160 MB** en macOS con una sesión. Debe confirmarse con `just e2e` en esas plataformas antes de fijar una meta.

La decisión final de la meta corresponde al humano.
