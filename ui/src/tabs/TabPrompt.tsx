import { useState, type JSX } from "react";
import type { TabController } from "./tabController";
import type { Tab } from "./tabState";

interface TabPromptProps {
  tab: Tab;
  controller: TabController;
}

const DEFAULT_SIZE = { rows: 24, cols: 80 };

export function TabPrompt({ tab, controller }: TabPromptProps): JSX.Element {
  const [secret, setSecret] = useState("");

  const status = tab.status;
  if (status.kind === "needs_password") {
    return (
      <div>
        <label>
          Contraseña
          <input type="password" value={secret} onChange={(e) => setSecret(e.target.value)} />
        </label>
        <button
          type="button"
          onClick={() => {
            const value = secret;
            setSecret("");
            void controller.connect({ password: value }, DEFAULT_SIZE);
          }}
        >
          Conectar
        </button>
      </div>
    );
  }

  if (status.kind === "needs_passphrase") {
    return (
      <div>
        <label>
          Passphrase
          <input type="password" value={secret} onChange={(e) => setSecret(e.target.value)} />
        </label>
        <button
          type="button"
          onClick={() => {
            const value = secret;
            setSecret("");
            void controller.connect({ passphrase: value }, DEFAULT_SIZE);
          }}
        >
          Conectar
        </button>
      </div>
    );
  }

  if (status.kind === "needs_host_key_approval") {
    return (
      <div>
        <p>{status.fingerprint}</p>
        <button
          type="button"
          onClick={() => {
            void controller.connect({ approvedFingerprint: status.fingerprint }, DEFAULT_SIZE);
          }}
        >
          Aceptar huella
        </button>
        <button type="button" onClick={() => void controller.close()}>
          Cancelar
        </button>
      </div>
    );
  }

  if (status.kind === "dropped") {
    const exit = status.exitStatus === null ? "desconocido" : String(status.exitStatus);
    return (
      <div>
        <p>Sesión terminada (código {exit})</p>
        <button type="button" onClick={() => void controller.reconnect(DEFAULT_SIZE)}>
          Reconectar
        </button>
      </div>
    );
  }

  if (status.kind === "error") {
    const canRetry = status.error.kind !== "host_key_changed";
    return (
      <div>
        <p>{status.error.message}</p>
        {canRetry && (
          <button type="button" onClick={() => void controller.reconnect(DEFAULT_SIZE)}>
            Reintentar
          </button>
        )}
      </div>
    );
  }

  if (status.kind === "connecting") {
    return <p>Conectando…</p>;
  }

  return <></>;
}
