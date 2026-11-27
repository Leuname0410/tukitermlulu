import { type JSX, type ReactNode, useCallback, useEffect, useRef, useState } from "react";
import {
  DEFAULT_SIDEBAR_WIDTH,
  SIDEBAR_KEYBOARD_STEP,
  clampWidth,
  loadWidth,
  saveWidth,
} from "./splitModel";

interface SplitPaneProps {
  sessionId?: number;
  left: ReactNode;
  right: ReactNode;
}

export function SplitPane({ sessionId, left, right }: SplitPaneProps): JSX.Element {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const widthRef = useRef(DEFAULT_SIDEBAR_WIDTH);
  const [width, setWidth] = useState(() => loadWidth(sessionId));

  useEffect(() => {
    const next = loadWidth(sessionId);
    widthRef.current = next;
    // Sincroniza el ancho almacenado cuando cambia la sesión activa.
    // eslint-disable-next-line react-hooks/set-state-in-effect
    setWidth(next);
  }, [sessionId]);

  const containerWidth = useCallback((): number => {
    const rect = containerRef.current?.getBoundingClientRect();
    return rect?.width ?? 0;
  }, []);

  const commitWidth = useCallback(
    (next: number): void => {
      const clamped = clampWidth(next, containerWidth());
      widthRef.current = clamped;
      setWidth(clamped);
      saveWidth(sessionId, clamped);
    },
    [containerWidth, sessionId],
  );

  const onPointerDown = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      const startX = event.clientX;
      const startWidth = widthRef.current;
      const separator = event.currentTarget;
      separator.setPointerCapture(event.pointerId);

      const onMove = (moveEvent: PointerEvent): void => {
        const delta = moveEvent.clientX - startX;
        const next = clampWidth(startWidth + delta, containerWidth());
        widthRef.current = next;
        setWidth(next);
      };

      const onUp = (): void => {
        separator.removeEventListener("pointermove", onMove);
        separator.removeEventListener("pointerup", onUp);
        separator.removeEventListener("pointercancel", onUp);
        if (
          typeof separator.hasPointerCapture === "function" &&
          separator.hasPointerCapture(event.pointerId)
        ) {
          separator.releasePointerCapture(event.pointerId);
        }
        saveWidth(sessionId, widthRef.current);
      };

      separator.addEventListener("pointermove", onMove);
      separator.addEventListener("pointerup", onUp);
      separator.addEventListener("pointercancel", onUp);
    },
    [containerWidth, sessionId],
  );

  const onKeyDown = useCallback(
    (event: React.KeyboardEvent<HTMLDivElement>) => {
      if (event.key === "ArrowRight") {
        event.preventDefault();
        commitWidth(widthRef.current + SIDEBAR_KEYBOARD_STEP);
      } else if (event.key === "ArrowLeft") {
        event.preventDefault();
        commitWidth(widthRef.current - SIDEBAR_KEYBOARD_STEP);
      }
    },
    [commitWidth],
  );

  return (
    <div
      ref={containerRef}
      style={{
        display: "grid",
        gridTemplateColumns: `${width}px 8px minmax(0, 1fr)`,
        minHeight: 0,
      }}
    >
      <div style={{ minWidth: 0, minHeight: 0 }}>{left}</div>
      <div
        role="separator"
        aria-orientation="vertical"
        aria-valuenow={Math.round(width)}
        aria-valuemin={180}
        tabIndex={0}
        onPointerDown={onPointerDown}
        onKeyDown={onKeyDown}
        style={{ cursor: "col-resize", touchAction: "none" }}
      />
      <div style={{ minWidth: 0, minHeight: 0 }}>{right}</div>
    </div>
  );
}
