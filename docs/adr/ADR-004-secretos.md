# ADR-004: Política de secretos (contraseñas y passphrases)

**Estado:** Propuesto (decisión tomada por el humano el 2026-11-25; falta aprobar el texto) · **Fecha:** 2026-11-25

## Contexto
La app maneja dos secretos: la contraseña SSH de una sesión y la passphrase que descifra una llave privada. Hay que equilibrar seguridad (no dejar secretos en disco) y comodidad (reconectar tras una caída sin volver a teclear).

## Decisión
- **Contraseña de sesión:** opcionalmente persistida en el keyring del SO (`SecretStore`) si el usuario marca "recordar". Se puede cambiar u olvidar; se borra al borrar la sesión; no se copia al duplicar.
- **Passphrase de llave:** **nunca se persiste** (ni SQLite, ni keyring, ni logs). Se pide al abrir la pestaña y se retiene **solo en memoria del backend**, por pestaña, en `SecretString` con zeroize:
  - reconectar en esa pestaña la reutiliza;
  - cerrar la pestaña o salir de la app la borra;
  - una passphrase incorrecta no se retiene;
  - editar la llave o `has_passphrase` de la sesión la invalida.
- Nunca viaja de vuelta a la UI ni se incluye en eventos, `Debug` o errores.
- **Pegar** en los campos de secreto está permitido (gestores de contraseñas); copiar desde ellos, no.

## Alternativas descartadas
- **Pedir la passphrase en cada reconexión:** más seguro, pero molesto en redes inestables.
- **Persistir la passphrase en el keyring:** convierte la llave cifrada en una llave en claro de facto para cualquiera con acceso a la sesión del SO.
- **Retener la llave descifrada en lugar de la passphrase:** mismo riesgo en memoria; se deja a criterio de la implementación (T-14/T-17) siempre que se cumpla zeroize.

## Consecuencias
- (+) Ningún secreto de llave en disco; reconexión sin fricción.
- (−) Un volcado de memoria del proceso mientras la pestaña está abierta podría exponer la passphrase (mitigado con zeroize y vida limitada a la pestaña).
