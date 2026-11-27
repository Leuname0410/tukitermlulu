import { useMemo, useState, type FormEvent, type JSX } from "react";
import {
  isCommandError,
  type CommandError,
  type FolderDto,
  type PasswordInput,
  type SessionDto,
  type SessionInput,
} from "../lib/api";

export interface SessionFormProps {
  initial?: SessionDto;
  folders: readonly FolderDto[];
  onSubmit(input: SessionInput, password: PasswordInput): Promise<void>;
  onCancel(): void;
}

function isValidPort(value: string): boolean {
  if (!/^\d+$/.test(value)) {
    return false;
  }
  const port = Number(value);
  return Number.isInteger(port) && port >= 1 && port <= 65535;
}

export function SessionForm(props: SessionFormProps): JSX.Element {
  const initial = props.initial;
  const initialAuth = initial?.auth;
  const [name, setName] = useState(initial?.name ?? "");
  const [host, setHost] = useState(initial?.host ?? "");
  const [port, setPort] = useState(String(initial?.port ?? 22));
  const [username, setUsername] = useState(initial?.username ?? "");
  const [folder, setFolder] = useState(initial?.folder === null || initial?.folder === undefined ? "" : String(initial.folder));
  const [authKind, setAuthKind] = useState<"password" | "key">(initialAuth?.kind ?? "password");
  const [keyPath, setKeyPath] = useState(initialAuth?.kind === "key" ? initialAuth.path : "");
  const [hasPassphrase, setHasPassphrase] = useState(
    initialAuth?.kind === "key" ? initialAuth.hasPassphrase : false,
  );
  const [remember, setRemember] = useState(
    initialAuth?.kind === "password" ? initialAuth.remember : false,
  );
  const [password, setPassword] = useState("");
  const [shellIntegration, setShellIntegration] = useState(initial?.shellIntegration ?? true);
  const passwordSaved = initial?.passwordSaved ?? false;
  const [forgetSaved, setForgetSaved] = useState(false);
  const [error, setError] = useState<CommandError | null>(null);

  const valid = useMemo(() => {
    if (name.trim() === "" || host.trim() === "" || username.trim() === "" || !isValidPort(port)) {
      return false;
    }
    return authKind !== "key" || keyPath.trim() !== "";
  }, [name, host, username, port, authKind, keyPath]);

  async function handleSubmit(event: FormEvent<HTMLFormElement>): Promise<void> {
    event.preventDefault();
    if (!valid) {
      return;
    }
    const input: SessionInput = {
      name: name.trim(),
      host: host.trim(),
      port: Number(port),
      username: username.trim(),
      auth:
        authKind === "password"
          ? { kind: "password", remember }
          : { kind: "key", path: keyPath.trim(), hasPassphrase },
      folder: folder === "" ? null : Number(folder),
      shellIntegration,
    };
    let passwordInput: PasswordInput = { kind: "keep" };
    if (authKind === "password") {
      if (password !== "") {
        passwordInput = { kind: "set", value: password };
      } else if (forgetSaved) {
        passwordInput = { kind: "forget" };
      }
    }

    setError(null);
    try {
      await props.onSubmit(input, passwordInput);
    } catch (cause: unknown) {
      setError(
        isCommandError(cause) ? cause : { kind: "io", message: "No se pudo guardar la sesión" },
      );
    } finally {
      setPassword("");
    }
  }

  return (
    <form
      aria-label={initial === undefined ? "Nueva sesión" : "Editar sesión"}
      onSubmit={(event) => {
        void handleSubmit(event);
      }}
    >
      {error !== null && <p role="alert">{error.message}</p>}
      <label>
        Nombre
        <input value={name} onChange={(event) => setName(event.target.value)} />
      </label>
      <label>
        Host
        <input value={host} onChange={(event) => setHost(event.target.value)} />
      </label>
      <label>
        Puerto
        <input type="number" min={1} max={65535} value={port} onChange={(event) => setPort(event.target.value)} />
      </label>
      <label>
        Usuario
        <input value={username} onChange={(event) => setUsername(event.target.value)} />
      </label>
      <label>
        Carpeta
        <select value={folder} onChange={(event) => setFolder(event.target.value)}>
          <option value="">Raíz</option>
          {props.folders.map((item) => (
            <option key={item.id} value={String(item.id)}>
              {item.name}
            </option>
          ))}
        </select>
      </label>
      <label>
        <input
          type="checkbox"
          checked={shellIntegration}
          onChange={(event) => setShellIntegration(event.target.checked)}
        />
        Seguir el directorio de la terminal (integración de shell)
      </label>
      <label>
        Método
        <select
          value={authKind}
          onChange={(event) => setAuthKind(event.target.value === "key" ? "key" : "password")}
        >
          <option value="password">Contraseña</option>
          <option value="key">Llave</option>
        </select>
      </label>
      {authKind === "password" ? (
        <>
          <label>
            Contraseña
            <input type="password" value={password} onChange={(event) => setPassword(event.target.value)} />
          </label>
          {initial !== undefined && passwordSaved && (
            <>
              <span>Contraseña guardada</span>
              <button type="button" onClick={() => setForgetSaved(true)}>
                Olvidar contraseña guardada
              </button>
            </>
          )}
          <label>
            <input type="checkbox" checked={remember} onChange={(event) => setRemember(event.target.checked)} />
            Recordar contraseña
          </label>
        </>
      ) : (
        <>
          <label>
            Ruta de llave
            <input value={keyPath} onChange={(event) => setKeyPath(event.target.value)} />
          </label>
          <label>
            <input
              type="checkbox"
              checked={hasPassphrase}
              onChange={(event) => setHasPassphrase(event.target.checked)}
            />
            La llave tiene passphrase
          </label>
        </>
      )}
      <button type="submit" disabled={!valid}>
        Guardar
      </button>
      <button type="button" onClick={props.onCancel}>
        Cancelar
      </button>
    </form>
  );
}
