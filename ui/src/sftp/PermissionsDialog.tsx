import { useEffect, useRef, useState, type JSX } from "react";
import { isCommandError, sftpChmod } from "../lib/api";

export interface PermissionsDialogProps {
  connectionId: number;
  path: string;
  mode: number;
  onSaved(): void;
  onClose(): void;
  onError(message: string): void;
}

const SLOTS = [
  { label: "Propietario lectura", bit: 0o400 },
  { label: "Propietario escritura", bit: 0o200 },
  { label: "Propietario ejecución", bit: 0o100 },
  { label: "Grupo lectura", bit: 0o040 },
  { label: "Grupo escritura", bit: 0o020 },
  { label: "Grupo ejecución", bit: 0o010 },
  { label: "Otros lectura", bit: 0o004 },
  { label: "Otros escritura", bit: 0o002 },
  { label: "Otros ejecución", bit: 0o001 },
] as const;

function parseOctal(text: string): number | null {
  const digits = text.trim().replace(/^0o/i, "");
  if (!/^[0-7]{1,4}$/.test(digits)) return null;
  const parsed = Number.parseInt(digits, 8);
  return Number.isFinite(parsed) && parsed >= 0 && parsed <= 0o7777 ? parsed : null;
}

function errorText(cause: unknown): string {
  if (isCommandError(cause)) return cause.message;
  return cause instanceof Error ? cause.message : "No se pudieron cambiar los permisos";
}

/** Diálogo de permisos Unix: casillas rwx y campo octal sincronizados en ambos sentidos. */
export function PermissionsDialog({
  connectionId,
  path,
  mode,
  onSaved,
  onClose,
  onError,
}: PermissionsDialogProps): JSX.Element {
  const dialogRef = useRef<HTMLDialogElement>(null);
  const onCloseRef = useRef(onClose);
  const [value, setValue] = useState(mode & 0o7777);
  const [octal, setOctal] = useState((mode & 0o7777).toString(8));
  const [validationError, setValidationError] = useState<string | null>(null);

  useEffect(() => {
    onCloseRef.current = onClose;
  }, [onClose]);

  useEffect(() => {
    const dialog = dialogRef.current;
    if (dialog === null || dialog.open) return;
    if (typeof dialog.showModal === "function") {
      dialog.showModal();
      return;
    }
    // jsdom no implementa showModal ni el cancel nativo de Escape.
    dialog.setAttribute("open", "");
    const handleKeyDown = (event: KeyboardEvent): void => {
      if (event.key === "Escape") onCloseRef.current();
    };
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, []);

  function toggle(bit: number, checked: boolean): void {
    const rwx = checked ? (value & 0o777) | bit : (value & 0o777) & ~bit;
    const next = (value & 0o7000) | rwx;
    setValue(next);
    setOctal(next.toString(8));
    setValidationError(null);
  }

  function changeOctal(text: string): void {
    setOctal(text);
    const parsed = parseOctal(text);
    if (parsed === null) {
      setValidationError("Introduce un modo octal entre 0 y 7777.");
      return;
    }
    setValidationError(null);
    setValue(parsed);
  }

  async function apply(): Promise<void> {
    const parsed = parseOctal(octal);
    if (parsed === null) {
      setValidationError("Introduce un modo octal entre 0 y 7777.");
      return;
    }
    try {
      await sftpChmod(connectionId, path, parsed);
      onSaved();
      onClose();
    } catch (cause) {
      onError(errorText(cause));
    }
  }

  return (
    <dialog
      ref={dialogRef}
      aria-label="Permisos"
      onClick={(event) => event.stopPropagation()}
      onContextMenu={(event) => event.preventDefault()}
      onCancel={(event) => {
        event.preventDefault();
        onClose();
      }}
    >
      {SLOTS.map((slot) => (
        <label key={slot.label}>
          <input
            type="checkbox"
            aria-label={slot.label}
            checked={(value & slot.bit) !== 0}
            onChange={(event) => toggle(slot.bit, event.target.checked)}
          />
          {slot.label}
        </label>
      ))}
      <label>
        Octal
        <input value={octal} onChange={(event) => changeOctal(event.target.value)} />
      </label>
      {validationError !== null && <p role="alert">{validationError}</p>}
      <button type="button" onClick={() => void apply()}>
        Aplicar
      </button>
      <button type="button" onClick={onClose}>
        Cancelar
      </button>
    </dialog>
  );
}
