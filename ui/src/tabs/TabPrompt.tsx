import type { JSX } from "react";
import { HostKeyDialog } from "../dialogs/HostKeyDialog";
import { SecretDialog } from "../dialogs/SecretDialog";
import type { TabController } from "./tabController";
import type { Tab } from "./tabState";

interface TabPromptProps {
  tab: Tab;
  controller: TabController;
  onPromptCancelled: (id: number) => void;
  onUpdateSavedPassword: (sessionId: number, value: string) => void;
}

const DEFAULT_SIZE = { rows: 24, cols: 80 };

export function TabPrompt({
  tab,
  controller,
  onPromptCancelled,
  onUpdateSavedPassword,
}: TabPromptProps): JSX.Element {
  const status = tab.status;
  const sessionId = tab.target.sessionId;

  function sendPassword(value: string, remember: boolean): void {
    void (async () => {
      await controller.connect({ password: value }, DEFAULT_SIZE);
      if (remember && sessionId !== undefined) {
        onUpdateSavedPassword(sessionId, value);
      }
    })();
  }

  function cancelSecret(): void {
    onPromptCancelled(tab.id);
  }

  function closeHostDialog(): void {
    void controller.close();
  }

  if (status.kind === "needs_password") {
    return (
      <SecretDialog
        kind="password"
        offerRemember={sessionId !== undefined}
        onSubmit={sendPassword}
        onCancel={cancelSecret}
      />
    );
  }

  if (status.kind === "needs_passphrase") {
    return (
      <SecretDialog
        kind="passphrase"
        offerRemember={false}
        onSubmit={(value) => {
          void controller.connect({ passphrase: value }, DEFAULT_SIZE);
        }}
        onCancel={cancelSecret}
      />
    );
  }

  if (status.kind === "needs_host_key_approval") {
    return (
      <HostKeyDialog
        host={tab.target.host}
        fingerprint={status.fingerprint}
        changed={false}
        onAccept={() => {
          void controller.connect({ approvedFingerprint: status.fingerprint }, DEFAULT_SIZE);
        }}
        onCancel={closeHostDialog}
      />
    );
  }

  if (status.kind === "error" && status.error.kind === "host_key_changed") {
    return (
      <HostKeyDialog
        host={tab.target.host}
        fingerprint={status.error.fingerprint ?? ""}
        changed
        onAccept={() => undefined}
        onCancel={closeHostDialog}
      />
    );
  }

  if (
    status.kind === "error" &&
    status.error.kind === "auth_rejected" &&
    sessionId !== undefined &&
    tab.target.auth.kind === "password" &&
    tab.target.auth.remember
  ) {
    return (
      <>
        <p>{status.error.message}</p>
        <SecretDialog kind="password" offerRemember onSubmit={sendPassword} onCancel={cancelSecret} />
      </>
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
    return (
      <div>
        <p>{status.error.message}</p>
        <button type="button" onClick={() => void controller.reconnect(DEFAULT_SIZE)}>
          Reintentar
        </button>
      </div>
    );
  }

  if (status.kind === "connecting") {
    return <p>Conectando…</p>;
  }

  return <></>;
}
