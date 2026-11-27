import { useEffect, useRef, useState, type JSX, type KeyboardEvent as ReactKeyboardEvent } from "react";
import type { EntryDto } from "../lib/api";
import {
  downloadsDir,
  enqueueDownload,
  isCommandError,
  sftpMkdir,
  sftpRemove,
  sftpRename,
} from "../lib/api";
import { joinPath, parentPath } from "./sidebarModel";
import { PermissionsDialog } from "./PermissionsDialog";

export interface EntryMenuProps {
  connectionId: number;
  directory: string;
  entry?: EntryDto;
  x: number;
  y: number;
  onClose(): void;
  onRefresh(): void;
  onError(message: string): void;
}

type Form = "none" | "rename" | "mkdir";

function errorText(cause: unknown): string {
  if (isCommandError(cause)) return cause.message;
  return cause instanceof Error ? cause.message : "No se pudo completar la operación";
}

/** Menú contextual de una entrada (o del fondo) con las acciones de archivo remoto. */
export function EntryMenu({
  connectionId,
  directory,
  entry,
  x,
  y,
  onClose,
  onRefresh,
  onError,
}: EntryMenuProps): JSX.Element {
  const [form, setForm] = useState<Form>("none");
  const [name, setName] = useState("");
  const [permissionsMode, setPermissionsMode] = useState(0);
  const [permissionsOpen, setPermissionsOpen] = useState(false);
  const closeRef = useRef(onClose);
  const position = { position: "fixed" as const, left: x, top: y };
  const path = entry?.path ?? directory;

  useEffect(() => {
    closeRef.current = onClose;
  }, [onClose]);

  useEffect(() => {
    const closeOnEscape = (event: KeyboardEvent): void => {
      if (event.key === "Escape") closeRef.current();
    };
    const closeOnOutsideClick = (): void => closeRef.current();
    document.addEventListener("keydown", closeOnEscape);
    document.addEventListener("click", closeOnOutsideClick);
    return () => {
      document.removeEventListener("keydown", closeOnEscape);
      document.removeEventListener("click", closeOnOutsideClick);
    };
  }, []);

  async function run(action: () => Promise<void>): Promise<void> {
    try {
      await action();
      onRefresh();
      onClose();
    } catch (cause) {
      onError(errorText(cause));
      onClose();
    }
  }

  function download(): void {
    const target = entry;
    if (target === undefined || target.isDir) return;
    void run(async () => {
      const localDir = await downloadsDir();
      await enqueueDownload(connectionId, target.path, localDir);
    });
  }

  function submitName(): void {
    const candidate = name.trim();
    if (
      candidate.length === 0 ||
      candidate.includes("/") ||
      candidate === "." ||
      candidate === ".."
    ) {
      onError("El nombre no puede estar vacío, contener '/' ni ser '.' o '..'.");
      return;
    }
    if (form === "rename") {
      const target = entry;
      if (target === undefined) return;
      const parent = parentPath(target.path);
      const to = parent === null ? candidate : joinPath(parent, candidate);
      void run(() => sftpRename(connectionId, target.path, to));
      return;
    }
    if (form === "mkdir") {
      void run(() => sftpMkdir(connectionId, joinPath(directory, candidate)));
    }
  }

  function remove(): void {
    const target = entry;
    if (target === undefined || !window.confirm(`¿Borrar ${target.name}?`)) return;
    void run(() => sftpRemove(connectionId, target.path));
  }

  function copyPath(): void {
    void navigator.clipboard.writeText(path).catch((cause: unknown) => onError(errorText(cause)));
    onClose();
  }

  function openPermissions(): void {
    if (entry === undefined || entry.permissions === undefined) return;
    setPermissionsMode(entry.permissions);
    setPermissionsOpen(true);
  }

  function onNameKeyDown(event: ReactKeyboardEvent<HTMLInputElement>): void {
    if (event.key === "Enter") {
      submitName();
    } else if (event.key === "Escape") {
      onClose();
    }
  }

  if (permissionsOpen && entry !== undefined) {
    return (
      <PermissionsDialog
        connectionId={connectionId}
        path={entry.path}
        mode={permissionsMode}
        onSaved={onRefresh}
        onClose={onClose}
        onError={onError}
      />
    );
  }

  if (form !== "none") {
    return (
      <div
        role="group"
        aria-label={form === "rename" ? "Renombrar entrada" : "Nueva carpeta"}
        style={position}
        onClick={(event) => event.stopPropagation()}
        onContextMenu={(event) => event.preventDefault()}
      >
        <input
          aria-label="Nuevo nombre"
          autoFocus
          value={name}
          onChange={(event) => setName(event.target.value)}
          onKeyDown={onNameKeyDown}
        />
        <button type="button" onClick={submitName}>
          Guardar
        </button>
        <button type="button" onClick={onClose}>
          Cancelar
        </button>
      </div>
    );
  }

  return (
    <div
      role="menu"
      style={position}
      onClick={(event) => event.stopPropagation()}
      onContextMenu={(event) => event.preventDefault()}
    >
      {entry !== undefined && !entry.isDir && (
        <button type="button" role="menuitem" onClick={download}>
          Descargar
        </button>
      )}
      {entry !== undefined && (
        <>
          <button
            type="button"
            role="menuitem"
            onClick={() => {
              setName(entry.name);
              setForm("rename");
            }}
          >
            Renombrar
          </button>
          <button type="button" role="menuitem" onClick={remove}>
            Borrar
          </button>
        </>
      )}
      <button
        type="button"
        role="menuitem"
        onClick={() => {
          setName("");
          setForm("mkdir");
        }}
      >
        Nueva carpeta
      </button>
      <button type="button" role="menuitem" onClick={copyPath}>
        Copiar ruta
      </button>
      {entry?.permissions !== undefined && (
        <button type="button" role="menuitem" onClick={openPermissions}>
          Permisos…
        </button>
      )}
    </div>
  );
}
