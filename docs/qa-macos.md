# QA manual en macOS

WKWebView no expone WebDriver, así que no hay e2e automatizado en macOS: el CI solo comprueba que la app arranca (`smoke-macos`). El resto se valida a mano con esta checklist.

## Entorno

- Versión de macOS: ______________________
- Chip: ☐ Intel ☐ Apple Silicon
- RAM observada en Monitor de Actividad: ______________________ MB

## Checklist

- [ ] Abrir el `.dmg` sin firmar (Gatekeeper): confirmar que se puede abrir desde «Abrir de todos modos» o clic derecho → Abrir, y que no aparece el aviso de «no se puede verificar el desarrollador» tras el primer arranque.
- [ ] Conectar con contraseña: autenticar contra el servidor de pruebas y ver el prompt/terminal listo.
- [ ] Conectar con llave: autenticar con llave privada (con y sin passphrase si aplica).
- [ ] Huella de host: primer acceso muestra la huella y la acepta; reconexión sin aviso; cambio de huella bloquea la conexión.
- [ ] Terminal: abrir `vim` y `htop`; copiar y pegar con Cmd+C / Cmd+V dentro de la terminal.
- [ ] SFTP: listar un directorio, subir un archivo arrastrándolo desde Finder, descargarlo, renombrarlo y cambiar permisos (chmod).
- [ ] Seguimiento de carpeta en `zsh` (shell por defecto de macOS): el prompt muestra la ruta y se actualiza al hacer `cd`.
- [ ] Exportar diagnóstico: generar el informe y comprobar que no contiene contraseñas, passphrases ni llaves.

## Resultado

- Probado por: ______________________
- Fecha: ______________________
- Incidencias: ______________________________________________________________
