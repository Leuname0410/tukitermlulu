export const DEFAULT_SIDEBAR_WIDTH = 280;
export const MIN_SIDEBAR_WIDTH = 180;
export const SIDEBAR_WIDTH_RATIO_MAX = 0.6;
export const SIDEBAR_KEYBOARD_STEP = 16;

export function clampWidth(width: number, _container: number): number {
  return width;
}

export function loadWidth(_sessionId?: number): number {
  return DEFAULT_SIDEBAR_WIDTH;
}

export function saveWidth(_sessionId: number | undefined, _width: number): void {}
