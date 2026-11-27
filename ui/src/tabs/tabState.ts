import type { CommandError, ConnectArgs, ConnectResponse } from "../lib/api";

/** Datos de conexión sin secretos ni tamaño del PTY. */
export type ConnectTarget = Omit<
  ConnectArgs,
  "password" | "passphrase" | "approvedFingerprint" | "rows" | "cols"
>;

export type TabStatus =
  | { kind: "connecting" }
  | { kind: "connected"; connectionId: number; channelId: number }
  | { kind: "needs_password" }
  | { kind: "needs_passphrase" }
  | { kind: "needs_host_key_approval"; fingerprint: string }
  | { kind: "dropped"; exitStatus: number | null }
  | { kind: "error"; error: CommandError };

export interface Tab {
  id: number;
  title: string;
  target: ConnectTarget;
  status: TabStatus;
  cwd?: string;
}

export type TabAction =
  | { type: "opened"; tab: Tab }
  | { type: "connecting"; id: number }
  | { type: "resolved"; id: number; response: ConnectResponse }
  | { type: "failed"; id: number; error: CommandError }
  | { type: "remote_closed"; id: number; exitStatus: number | null }
  | { type: "cwd"; id: number; path: string }
  | { type: "closed"; id: number };

function statusFromResponse(response: ConnectResponse): TabStatus {
  switch (response.kind) {
    case "connected":
      return {
        kind: "connected",
        connectionId: response.connectionId,
        channelId: response.channelId,
      };
    case "needs_host_key_approval":
      return { kind: response.kind, fingerprint: response.fingerprint };
    case "needs_password":
    case "needs_passphrase":
      return { kind: response.kind };
  }
}

function updateTab(tabs: readonly Tab[], id: number, update: (tab: Tab) => Tab): Tab[] {
  let found = false;
  const next = tabs.map((tab) => {
    if (tab.id !== id) {
      return tab;
    }
    found = true;
    return update(tab);
  });
  return found ? next : (tabs as Tab[]);
}

function withStatus(tabs: readonly Tab[], id: number, status: TabStatus): Tab[] {
  return updateTab(tabs, id, (tab) => ({ ...tab, status }));
}

/** Máquina de estados pura de las pestañas; ignora acciones de ids desconocidos. */
export function tabsReducer(tabs: readonly Tab[], action: TabAction): Tab[] {
  switch (action.type) {
    case "opened":
      return [...tabs, action.tab];
    case "connecting":
      return withStatus(tabs, action.id, { kind: "connecting" });
    case "resolved":
      return withStatus(tabs, action.id, statusFromResponse(action.response));
    case "failed":
      return withStatus(tabs, action.id, { kind: "error", error: action.error });
    case "remote_closed":
      return withStatus(tabs, action.id, {
        kind: "dropped",
        exitStatus: action.exitStatus,
      });
    case "cwd":
      return updateTab(tabs, action.id, (tab) => ({ ...tab, cwd: action.path }));
    case "closed": {
      const next = tabs.filter((tab) => tab.id !== action.id);
      return next.length === tabs.length ? (tabs as Tab[]) : next;
    }
  }
}
