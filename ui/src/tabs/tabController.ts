import {
  closeTab,
  connect as apiConnect,
  disconnect,
  isCommandError,
  type ConnectArgs,
  type TerminalHandlers,
} from "../lib/api";
import type { TerminalSource } from "../terminal/useTerminal";
import type { Tab, TabAction } from "./tabState";

export interface ConnectSecrets {
  password?: string;
  passphrase?: string;
  approvedFingerprint?: string;
}

export interface TerminalSize {
  rows: number;
  cols: number;
}

export interface TabController extends TerminalSource {
  connect(secrets: ConnectSecrets, size: TerminalSize): Promise<void>;
  reconnect(size: TerminalSize): Promise<void>;
  close(): Promise<void>;
}

// ponytail: tope de 1 MiB para no acumular salida infinita antes de montar la terminal.
const MAX_BUFFERED_BYTES = 1024 * 1024;

/** Controla la sesión de una pestaña sin mezclar bytes del PTY con estado de React. */
export function createTabController(
  tab: Pick<Tab, "id" | "target">,
  dispatch: (action: TabAction) => void,
): TabController {
  let connectionId: number | null = null;
  let sink: ((bytes: Uint8Array) => void) | null = null;
  let pending: Uint8Array[] = [];
  let pendingBytes = 0;

  function trimPending(): void {
    let excess = pendingBytes - MAX_BUFFERED_BYTES;
    while (excess > 0 && pending.length > 0) {
      const oldest = pending.shift();
      if (oldest === undefined) {
        break;
      }
      if (oldest.byteLength > excess) {
        pending.unshift(oldest.slice(excess));
        pendingBytes -= excess;
        break;
      }
      pendingBytes -= oldest.byteLength;
      excess -= oldest.byteLength;
    }
  }

  function enqueue(bytes: Uint8Array): void {
    const chunk =
      bytes.byteLength > MAX_BUFFERED_BYTES
        ? bytes.slice(bytes.byteLength - MAX_BUFFERED_BYTES)
        : bytes;
    pending.push(chunk);
    pendingBytes += chunk.byteLength;
    trimPending();
  }

  const handlers: TerminalHandlers = {
    onData: (bytes) => {
      if (sink !== null) {
        sink(bytes);
        return;
      }
      enqueue(bytes);
    },
    onClosed: (exitStatus) => {
      connectionId = null;
      dispatch({ type: "remote_closed", id: tab.id, exitStatus });
    },
  };

  async function startConnect(secrets: ConnectSecrets, size: TerminalSize): Promise<void> {
    dispatch({ type: "connecting", id: tab.id });
    const args: ConnectArgs = { ...tab.target, rows: size.rows, cols: size.cols };
    if (secrets.password !== undefined) {
      args.password = secrets.password;
    }
    if (secrets.passphrase !== undefined) {
      args.passphrase = secrets.passphrase;
    }
    if (secrets.approvedFingerprint !== undefined) {
      args.approvedFingerprint = secrets.approvedFingerprint;
    }
    try {
      const response = await apiConnect(tab.id, args, handlers);
      connectionId = response.kind === "connected" ? response.connectionId : null;
      dispatch({ type: "resolved", id: tab.id, response });
    } catch (error: unknown) {
      const commandError = isCommandError(error)
        ? error
        : { kind: "io", message: "Error inesperado" };
      connectionId = null;
      dispatch({ type: "failed", id: tab.id, error: commandError });
    }
  }

  const attach = (onData: (bytes: Uint8Array) => void): (() => void) => {
    sink = onData;
    const buffered = pending;
    pending = [];
    pendingBytes = 0;
    for (const bytes of buffered) {
      onData(bytes);
    }
    return () => {
      if (sink === onData) {
        sink = null;
      }
    };
  };

  async function close(): Promise<void> {
    if (connectionId !== null) {
      try {
        await disconnect(connectionId);
      } catch {
        // Un fallo al desconectar no impide cerrar la pestaña.
      }
    }
    try {
      await closeTab(tab.id);
    } catch {
      // Un fallo al cerrar en el backend no impide cerrar la pestaña localmente.
    }
    connectionId = null;
    sink = null;
    pending = [];
    pendingBytes = 0;
    dispatch({ type: "closed", id: tab.id });
  }

  return {
    connect: startConnect,
    reconnect: (size) => startConnect({}, size),
    close,
    attach,
  };
}
