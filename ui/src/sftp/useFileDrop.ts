import { useEffect, useRef, useState, type RefObject } from "react";
import {
  enqueueUpload,
  isCommandError,
  onFileDrop,
  onTransfers,
  type FileDropEvent,
  type TransferDto,
} from "../lib/api";
import { parentPath } from "./sidebarModel";

interface Point {
  x: number;
  y: number;
}

export interface FileDropOptions {
  connectionId: number;
  currentPath: string;
  sidebarRef: RefObject<HTMLElement | null>;
  onError(message: string): void;
  onRefresh(): void;
}

/** Convierte coordenadas físicas de Tauri a píxeles CSS. */
export function toCssPoint(x: number, y: number, ratio: number): Point {
  const scale = ratio > 0 ? ratio : 1;
  return { x: x / scale, y: y / scale };
}

/** Comprueba si el punto cae dentro de un sidebar visible. */
function isInside(element: HTMLElement | null, point: Point): boolean {
  if (element === null) return false;
  const rect = element.getBoundingClientRect();
  if (rect.width <= 0 || rect.height <= 0) return false;
  return (
    point.x >= rect.left && point.x <= rect.right && point.y >= rect.top && point.y <= rect.bottom
  );
}

function messageFrom(cause: unknown): string {
  if (isCommandError(cause)) return cause.message;
  return cause instanceof Error ? cause.message : "No se pudo encolar la subida";
}

/** Detecta el rechazo del backend a una ruta que no es un archivo. */
function isFolder(message: string): boolean {
  return /must be a file|is a directory|carpeta|directorio/i.test(message);
}

/** Comprueba si una subida terminó en la carpeta que muestra el sidebar. */
function finishedHere(job: TransferDto, connectionId: number, path: string): boolean {
  return (
    job.connectionId === connectionId &&
    job.direction === "upload" &&
    job.state === "done" &&
    parentPath(job.remote) === path
  );
}

/** Encola los archivos soltados y mantiene el resaltado del sidebar activo. */
export function useFileDrop({
  connectionId,
  currentPath,
  sidebarRef,
  onError,
  onRefresh,
}: FileDropOptions): boolean {
  const [dropActive, setDropActive] = useState(false);
  const latest = useRef({ connectionId, currentPath, onError, onRefresh });

  useEffect(() => {
    latest.current = { connectionId, currentPath, onError, onRefresh };
  }, [connectionId, currentPath, onError, onRefresh]);

  useEffect(() => {
    let cancelled = false;
    let stopDrops: (() => void) | undefined;
    let stopTransfers: (() => void) | undefined;
    let lastRefresh = 0;

    const handleDrop = (event: FileDropEvent): void => {
      if (event.kind === "leave") {
        setDropActive(false);
        return;
      }
      const point = toCssPoint(event.x, event.y, window.devicePixelRatio);
      const inside = isInside(sidebarRef.current, point);
      setDropActive(event.kind === "over" && inside);
      if (event.kind !== "drop" || !inside) return;

      void (async () => {
        for (const path of event.paths) {
          try {
            await enqueueUpload(latest.current.connectionId, path, latest.current.currentPath);
          } catch (cause) {
            const message = messageFrom(cause);
            latest.current.onError(
              isFolder(message) ? "Las carpetas aún no se pueden subir" : message,
            );
          }
        }
      })();
    };

    const handleTransfers = (jobs: TransferDto[]): void => {
      if (!jobs.some((job) => finishedHere(job, latest.current.connectionId, latest.current.currentPath))) {
        return;
      }
      const now = Date.now();
      if (now - lastRefresh < 1000) return;
      lastRefresh = now;
      latest.current.onRefresh();
    };

    void (async () => {
      try {
        const [unsubscribeDrops, unsubscribeTransfers] = await Promise.all([
          onFileDrop(handleDrop),
          onTransfers(handleTransfers),
        ]);
        if (cancelled) {
          unsubscribeDrops();
          unsubscribeTransfers();
          return;
        }
        stopDrops = unsubscribeDrops;
        stopTransfers = unsubscribeTransfers;
      } catch {
        // Sin webview Tauri no hay eventos de arrastre; la UI sigue funcionando.
      }
    })();

    return () => {
      cancelled = true;
      stopDrops?.();
      stopTransfers?.();
    };
  }, [sidebarRef]);

  return dropActive;
}
