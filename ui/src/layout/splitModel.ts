export const DEFAULT_SIDEBAR_WIDTH = 280;
export const MIN_SIDEBAR_WIDTH = 180;
export const SIDEBAR_WIDTH_RATIO_MAX = 0.6;
export const SIDEBAR_KEYBOARD_STEP = 16;

function storageKey(sessionId?: number): string {
  return sessionId === undefined
    ? "tuki.sidebarWidth.default"
    : `tuki.sidebarWidth.${sessionId}`;
}

export function clampWidth(width: number, container: number): number {
  const maxByRatio = Math.floor(container * SIDEBAR_WIDTH_RATIO_MAX);
  const maxWidth = Math.max(MIN_SIDEBAR_WIDTH, maxByRatio);
  if (width < MIN_SIDEBAR_WIDTH) {
    return MIN_SIDEBAR_WIDTH;
  }
  if (width > maxWidth) {
    return maxWidth;
  }
  return width;
}

export function loadWidth(sessionId?: number): number {
  try {
    const raw = window.localStorage.getItem(storageKey(sessionId));
    if (raw === null) {
      return DEFAULT_SIDEBAR_WIDTH;
    }
    const parsed = Number(raw);
    if (!Number.isFinite(parsed)) {
      return DEFAULT_SIDEBAR_WIDTH;
    }
    return parsed;
  } catch {
    return DEFAULT_SIDEBAR_WIDTH;
  }
}

export function saveWidth(sessionId: number | undefined, width: number): void {
  try {
    window.localStorage.setItem(storageKey(sessionId), String(width));
  } catch {
    // localStorage no disponible: mantener valor por defecto en memoria.
  }
}
