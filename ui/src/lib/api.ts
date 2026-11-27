import { Channel, invoke } from "@tauri-apps/api/core";

export type AuthArgs =
  | { kind: "password"; remember: boolean }
  | { kind: "key"; path: string; hasPassphrase: boolean };

export interface ConnectArgs {
  host: string;
  port: number;
  username: string;
  auth: AuthArgs;
  password?: string;
  passphrase?: string;
  approvedFingerprint?: string;
  rows: number;
  cols: number;
}

export type ConnectResponse =
  | { kind: "connected"; connectionId: number; channelId: number }
  | { kind: "needs_host_key_approval"; fingerprint: string }
  | { kind: "needs_password" }
  | { kind: "needs_passphrase" };

export interface CommandError {
  kind: string;
  message: string;
  fingerprint?: string;
}

export interface TerminalHandlers {
  onData: (bytes: Uint8Array) => void;
  onClosed: (exitStatus: number | null) => void;
}

// El backend serializa las variantes de ConnectResponse en snake_case y sus campos
// como connection_id / channel_id; aquí se normalizan a camelCase para la UI.
// ConnectArgs y CommandError ya son camelCase. Tauri convierte las claves de invoke
// (channelId, onOutput, ...) a los parámetros snake_case de los comandos Rust.
type UnknownRecord = Record<string, unknown>;

function asRecord(value: unknown): UnknownRecord {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw new Error("El backend devolvió una respuesta inesperada de la terminal");
  }
  return value as UnknownRecord;
}

function readNumber(value: unknown, field: string): number {
  if (typeof value !== "number" || !Number.isInteger(value)) {
    throw new Error(`El backend devolvió ${field} inválido`);
  }
  return value;
}

function parseConnectResponse(value: unknown): ConnectResponse {
  const response = asRecord(value);
  switch (response.kind) {
    case "connected":
      return {
        kind: "connected",
        connectionId: readNumber(response.connection_id, "connection_id"),
        channelId: readNumber(response.channel_id, "channel_id"),
      };
    case "needs_host_key_approval":
      if (typeof response.fingerprint !== "string") {
        throw new Error("El backend devolvió una huella de host inválida");
      }
      return { kind: "needs_host_key_approval", fingerprint: response.fingerprint };
    case "needs_password":
      return { kind: "needs_password" };
    case "needs_passphrase":
      return { kind: "needs_passphrase" };
    default:
      throw new Error("El backend devolvió una respuesta de conexión desconocida");
  }
}

function parseExitStatus(value: unknown): number | null {
  const message = asRecord(value);
  const status = message.exit_status;
  if (status === null) {
    return null;
  }
  return readNumber(status, "exit_status");
}

/** Conecta una pestaña y entrega la salida del PTY a los handlers, sin estado de React. */
export async function connect(
  tabId: number,
  args: ConnectArgs,
  handlers: TerminalHandlers,
): Promise<ConnectResponse> {
  const onOutput = new Channel<unknown>();
  const onClosed = new Channel<unknown>();

  onOutput.onmessage = (message) => {
    if (!(message instanceof ArrayBuffer)) {
      return;
    }
    handlers.onData(new Uint8Array(message));
  };
  onClosed.onmessage = (message) => {
    handlers.onClosed(parseExitStatus(message));
  };

  const response = await invoke<unknown>("connect", { tabId, args, onOutput, onClosed });
  return parseConnectResponse(response);
}

/** Escribe bytes raw en el PTY indicado por el header x-channel-id. */
export async function terminalWrite(channelId: number, data: Uint8Array): Promise<void> {
  await invoke("terminal_write", data, {
    headers: { "x-channel-id": String(channelId) },
  });
}

/** Ajusta el tamaño del PTY indicado. */
export async function terminalResize(channelId: number, rows: number, cols: number): Promise<void> {
  await invoke("terminal_resize", { channelId, rows, cols });
}

/** Cierra una conexión y sus PTY asociados. */
export async function disconnect(connectionId: number): Promise<void> {
  await invoke("disconnect", { connectionId });
}

/** Olvida los secretos retenidos por el backend para la pestaña. */
export async function closeTab(tabId: number): Promise<void> {
  await invoke("close_tab", { tabId });
}

/** Estrecha un error IPC; Option::None se serializa como fingerprint: null. */
export function isCommandError(value: unknown): value is CommandError {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    return false;
  }
  const candidate = value as UnknownRecord;
  const fingerprint =
    candidate.fingerprint === undefined ||
    candidate.fingerprint === null ||
    typeof candidate.fingerprint === "string";
  return (
    fingerprint && typeof candidate.kind === "string" && typeof candidate.message === "string"
  );
}
