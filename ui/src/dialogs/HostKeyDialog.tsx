import { useEffect, useRef, type JSX } from "react";

export interface HostKeyDialogProps {
  fingerprint: string;
  changed: boolean;
  host: string;
  onAccept(): void;
  onCancel(): void;
}

/** Diálogo modal de huella: la nueva se puede aceptar; la cambiada solo se avisa. */
export function HostKeyDialog({
  fingerprint,
  changed,
  host,
  onAccept,
  onCancel,
}: HostKeyDialogProps): JSX.Element {
  const dialogRef = useRef<HTMLDialogElement>(null);
  const onCancelRef = useRef(onCancel);

  useEffect(() => {
    onCancelRef.current = onCancel;
  }, [onCancel]);

  useEffect(() => {
    const dialog = dialogRef.current;
    if (dialog === null || dialog.open) {
      return;
    }
    if (typeof dialog.showModal === "function") {
      dialog.showModal();
      return;
    }
    // jsdom no implementa showModal ni el cancel nativo de Escape.
    dialog.setAttribute("open", "");
    const handleKeyDown = (event: KeyboardEvent): void => {
      if (event.key === "Escape") {
        onCancelRef.current();
      }
    };
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, []);

  return (
    <dialog
      ref={dialogRef}
      aria-label={changed ? "Huella cambiada" : "Huella del host"}
      onCancel={(event) => {
        event.preventDefault();
        onCancel();
      }}
    >
      {changed ? (
        <p>La huella del host cambió</p>
      ) : (
        <p>¿Confiar en la huella de {host}?</p>
      )}
      <p>{fingerprint}</p>
      {changed ? (
        <button type="button" onClick={onCancel}>
          Cerrar
        </button>
      ) : (
        <>
          <button type="button" onClick={onAccept}>
            Aceptar
          </button>
          <button type="button" onClick={onCancel}>
            Cancelar
          </button>
        </>
      )}
    </dialog>
  );
}
