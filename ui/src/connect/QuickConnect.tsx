import { useMemo, useState, type FormEvent, type JSX } from "react";
import type { ConnectTarget } from "../tabs/tabState";

interface QuickConnectProps {
  onSubmit: (target: ConnectTarget) => void;
}

function isValidPort(value: string): boolean {
  if (!/^\d+$/.test(value)) {
    return false;
  }
  const port = Number(value);
  return port >= 1 && port <= 65535;
}

export function QuickConnect({ onSubmit }: QuickConnectProps): JSX.Element {
  const [host, setHost] = useState("");
  const [port, setPort] = useState("22");
  const [username, setUsername] = useState("");
  const [authKind, setAuthKind] = useState<"password" | "key">("password");
  const [keyPath, setKeyPath] = useState("");
  const [remember, setRemember] = useState(false);

  const valid = useMemo(() => {
    if (host.trim() === "" || username.trim() === "") {
      return false;
    }
    if (!isValidPort(port)) {
      return false;
    }
    if (authKind === "key" && keyPath.trim() === "") {
      return false;
    }
    return true;
  }, [host, port, username, authKind, keyPath]);

  function handleSubmit(event: FormEvent<HTMLFormElement>): void {
    event.preventDefault();
    if (!valid) {
      return;
    }
    const target: ConnectTarget = {
      host: host.trim(),
      port: Number(port),
      username: username.trim(),
      auth:
        authKind === "password"
          ? { kind: "password", remember }
          : { kind: "key", path: keyPath.trim(), hasPassphrase: false },
    };
    onSubmit(target);
  }

  return (
    <form aria-label="Conexión rápida" onSubmit={handleSubmit}>
      <label>
        Host
        <input value={host} onChange={(e) => setHost(e.target.value)} />
      </label>
      <label>
        Puerto
        <input type="number" min={1} max={65535} value={port} onChange={(e) => setPort(e.target.value)} />
      </label>
      <label>
        Usuario
        <input value={username} onChange={(e) => setUsername(e.target.value)} />
      </label>
      <label>
        Método
        <select value={authKind} onChange={(e) => setAuthKind(e.target.value === "key" ? "key" : "password")}>
          <option value="password">Password</option>
          <option value="key">Key</option>
        </select>
      </label>
      {authKind === "key" && (
        <label>
          Ruta de llave
          <input value={keyPath} onChange={(e) => setKeyPath(e.target.value)} />
        </label>
      )}
      <label>
        <input type="checkbox" checked={remember} onChange={(e) => setRemember(e.target.checked)} />
        Recordar
      </label>
      <button type="submit" disabled={!valid}>
        Conectar
      </button>
    </form>
  );
}
