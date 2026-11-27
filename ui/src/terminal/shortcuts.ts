export type Platform = "mac" | "other";

/** Detecta la plataforma a partir del user agent para elegir los atajos de portapapeles. */
export function detectPlatform(userAgent: string): Platform {
  return userAgent.includes("Mac") ? "mac" : "other";
}

export type ClipboardAction = "copy" | "paste" | null;

type KeyLike = Pick<KeyboardEvent, "key" | "ctrlKey" | "metaKey" | "shiftKey" | "type">;

/**
 * Traduce un evento de teclado en una acción del portapapeles.
 * Ctrl+C sin Shift se deja al remoto (SIGINT) y Solo keydown se considera atajo.
 */
export function clipboardAction(e: KeyLike, platform: Platform): ClipboardAction {
  if (e.type !== "keydown") {
    return null;
  }

  if (platform === "mac") {
    if (!e.metaKey || e.ctrlKey) {
      return null;
    }
  } else if (!e.ctrlKey || e.metaKey || !e.shiftKey) {
    return null;
  }

  switch (e.key.toLowerCase()) {
    case "c":
      return "copy";
    case "v":
      return "paste";
    default:
      return null;
  }
}
