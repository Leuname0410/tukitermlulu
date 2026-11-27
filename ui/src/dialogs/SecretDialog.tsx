import { useEffect, useRef, useState, type JSX } from "react";

export interface SecretDialogProps {
  kind: "password" | "passphrase";
  offerRemember: boolean;
  onSubmit(value: string, remember: boolean): void;
  onCancel(): void;
}

/** Diálogo modal de secreto: se permite pegar, no copiar/cortar y nunca se prellena. */
export function SecretDialog({
  kind,
  offerRemember,
  onSubmit,
  onCancel,
}: SecretDialogProps): JSX.Element {
  const dialogRef = useRef<HTMLDialogElement>(null);
  const onCancelRef = useRef(onCancel);
  const [value, setValue] = useState("");
  const [remember, setRemember] = useState(false);
  const showRemember = kind === "password" && offerRemember;
  const label = kind === "passphrase" ? "Passphrase" : "Contraseña";

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

  function submit(): void {
    const submitted = value;
    setValue("");
    onSubmit(submitted, showRemember && remember);
  }

  return (
    <dialog
      ref={dialogRef}
      aria-label="Diálogo de secreto"
      onCancel={(event) => {
        event.preventDefault();
        onCancel();
      }}
    >
      <label>
        {label}
        <input
          type="password"
          autoComplete="off"
          value={value}
          onChange={(event) => setValue(event.target.value)}
          onCopy={(event) => event.preventDefault()}
          onCut={(event) => event.preventDefault()}
        />
      </label>
      {showRemember && (
        <label>
          <input
            type="checkbox"
            checked={remember}
            onChange={(event) => setRemember(event.target.checked)}
          />
          Recordar
        </label>
      )}
      <button type="button" onClick={submit}>
        Conectar
      </button>
      <button type="button" onClick={onCancel}>
        Cancelar
      </button>
    </dialog>
  );
}
