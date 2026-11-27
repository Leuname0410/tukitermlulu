# ADR-006: Seguimiento del directorio remoto por inyección de shell integration en el PTY

**Estado:** Propuesto (pendiente de aprobación humana) · **Fecha:** 2026-11-26

## Contexto
Requisito (decisiones D1–D3 del humano): al abrir una sesión arrancan la terminal y el sidebar SFTP a la izquierda; el sidebar **sigue** el directorio de la shell (`cd /var/log` → el sidebar muestra `/var/log`) y una casilla permite dejar de seguirlo. Debe funcionar a largo plazo con servidores Linux, macOS y Windows (OpenSSH para Windows), sin configurar nada en el servidor.
Solo la shell interactiva conoce su directorio. Un canal `exec` aparte arranca otra shell en `$HOME`, así que no sirve para preguntarle (por eso T-29 se descarta). El protocolo estándar es **OSC 7** (`ESC ] 7 ; file://host/ruta BEL`), que ya emiten muchas configuraciones y terminales modernas.

## Decisión
1. **Parser OSC 7** sobre los bytes del PTY (T-27), sin alterar lo que llega a xterm.
2. **Detección de la shell** por un canal `exec` aparte antes de inyectar nada: `echo "$0|$SHELL|$PSVersionTable"` (o equivalente) clasifica como `bash`, `zsh`, `fish`, `pwsh`, `cmd` u `unknown`. Si es `unknown`, no se inyecta.
3. **Inyección automática, activada por defecto**, con una línea específica para cada shell, que añade la emisión de OSC 7 al prompt sin reemplazar el del usuario:
   - bash: `PROMPT_COMMAND` (se añade al existente)
   - zsh: `precmd_functions+=(...)`
   - fish: `function --on-variable PWD`
   - PowerShell: envuelve la función `prompt` existente
   - cmd.exe: `prompt $E]7;file:///$P$E\%PROMPT%` (`$P` = directorio actual)
4. **Sin eco visible:**
   - Se inyecta cuando la salida del shell lleva ~300 ms en silencio, es decir, tras el MOTD y el primer prompt.
   - El backend oculta la salida desde la inyección hasta un marcador único (`ESC ] 777 ; tuki-ready BEL`) que la propia línea imprime, con un timeout de 2 s que restablece la salida pase lo que pase.
   - La línea termina limpiando la línea actual (`\r ESC[K`) para que el prompt se redibuje una sola vez.
5. **Sin historial:**
   - En bash y zsh la línea empieza con espacio y además se borra del historial (`history -d` / `fc -p`).
   - En PowerShell se usa `Remove-History` o `Set-PSReadLineOption -AddToHistoryHandler`.
   - En cmd.exe no hay historial persistente.
6. Opción por sesión para desactivar la inyección (servidores restringidos, cuentas de solo `sftp`). La casilla "Seguir terminal" del sidebar es independiente y está activada por defecto.

## Alternativas descartadas
- **Variables de entorno por SSH (`SendEnv`/`setenv`):** requiere `AcceptEnv` en el servidor, que casi nunca está activo.
- **Editar los archivos rc remotos (`~/.bashrc`):** modifica la cuenta del usuario de forma persistente e invasiva.
- **`pwd` por un canal `exec`:** devuelve el directorio de otra shell, no el de la interactiva.
- **`/proc/<pid>/cwd`:** solo Linux, y encontrar el PID del PTY no es fiable.

## Consecuencias
- (+) Funciona sin tocar el servidor en las shells habituales de los 3 SO, sin eco ni historial.
- (+) Si la shell ya emite OSC 7, se aprovecha igualmente.
- (−) Mantener 5 líneas de inyección, una por shell. Si el usuario redefine el prompt después (p. ej. `exec bash`, `su`), se pierde el seguimiento hasta reconectar. Hay que documentarlo.
- (−) La ocultación temporal de la salida puede tapar hasta 2 s de salida legítima si el marcador no llega.
- (−) El contenedor de tests necesita `bash`, `zsh` y `fish` (Alpine trae `ash`). PowerShell y cmd se prueban a mano en Windows Server.
