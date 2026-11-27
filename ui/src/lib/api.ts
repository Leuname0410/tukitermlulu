import { Channel, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";

export type AuthArgs =
  | { kind: "password"; remember: boolean }
  | { kind: "key"; path: string; hasPassphrase: boolean };

export interface ConnectArgs {
  sessionId?: number;
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

export type AuthDto =
  | { kind: "password"; remember: boolean }
  | { kind: "key"; path: string; hasPassphrase: boolean };

export interface SessionInput {
  name: string;
  host: string;
  port: number;
  username: string;
  auth: AuthDto;
  folder?: number | null;
}

export type PasswordInput =
  | { kind: "keep" }
  | { kind: "set"; value: string }
  | { kind: "forget" };

export interface SessionDto {
  id: number;
  name: string;
  host: string;
  port: number;
  username: string;
  auth: AuthDto;
  folder: number | null;
  passwordSaved: boolean;
}

export interface FolderDto {
  id: number;
  name: string;
  parent: number | null;
}

export interface FolderInput {
  id?: number;
  name: string;
  parent?: number | null;
}

export interface CommandError {
  kind: string;
  message: string;
  fingerprint?: string;
}

export type TransferDirection = "download" | "upload";

export type TransferState =
  | "queued"
  | "running"
  | "done"
  | "failed"
  | "cancelled";

export interface TransferDto {
  id: number;
  connectionId: number;
  direction: TransferDirection;
  remote: string;
  local: string;
  state: TransferState;
  transferred: number;
  total: number | null;
  error: string | null;
}

export interface EntryDto {
  name: string;
  path: string;
  isDir: boolean;
  size: number;
}

/** Evento de arrastre del SO, con coordenadas físicas relativas al webview. */
export type FileDropEvent =
  | { kind: "over"; x: number; y: number }
  | { kind: "drop"; paths: string[]; x: number; y: number }
  | { kind: "leave" };

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

function readString(value: unknown, field: string): string {
  if (typeof value !== "string") {
    throw new Error(`El backend devolvió ${field} inválido`);
  }
  return value;
}

function readBoolean(value: unknown, field: string): boolean {
  if (typeof value !== "boolean") {
    throw new Error(`El backend devolvió ${field} inválido`);
  }
  return value;
}

function readOptionalNumber(value: unknown, field: string): number | null {
  if (value === null || value === undefined) {
    return null;
  }
  return readNumber(value, field);
}

function parseAuth(value: unknown): AuthDto {
  const auth = asRecord(value);
  switch (auth.kind) {
    case "password":
      return { kind: "password", remember: readBoolean(auth.remember, "remember") };
    case "key":
      return {
        kind: "key",
        path: readString(auth.path, "path"),
        hasPassphrase: readBoolean(auth.hasPassphrase, "hasPassphrase"),
      };
    default:
      throw new Error("El backend devolvió un método de autenticación desconocido");
  }
}

function parseSessionDto(value: unknown): SessionDto {
  const session = asRecord(value);
  return {
    id: readNumber(session.id, "id"),
    name: readString(session.name, "name"),
    host: readString(session.host, "host"),
    port: readNumber(session.port, "port"),
    username: readString(session.username, "username"),
    auth: parseAuth(session.auth),
    folder: readOptionalNumber(session.folder, "folder"),
    passwordSaved: readBoolean(session.passwordSaved, "passwordSaved"),
  };
}

function parseSessionList(value: unknown): SessionDto[] {
  if (!Array.isArray(value)) {
    throw new Error("El backend devolvió una lista de sesiones inválida");
  }
  return value.map(parseSessionDto);
}

function parseFolderDto(value: unknown): FolderDto {
  const folder = asRecord(value);
  return {
    id: readNumber(folder.id, "id"),
    name: readString(folder.name, "name"),
    parent: readOptionalNumber(folder.parent, "parent"),
  };
}

function parseFolderList(value: unknown): FolderDto[] {
  if (!Array.isArray(value)) {
    throw new Error("El backend devolvió una lista de carpetas inválida");
  }
  return value.map(parseFolderDto);
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

function readNullableNumber(value: unknown, field: string): number | null {
  if (value === null) {
    return null;
  }
  return readNumber(value, field);
}

function readNullableString(value: unknown, field: string): string | null {
  if (value === null) {
    return null;
  }
  return readString(value, field);
}

function parseTransfer(value: unknown): TransferDto {
  const job = asRecord(value);
  const direction = job.direction;
  if (direction !== "download" && direction !== "upload") {
    throw new Error("El backend devolvió una dirección de transferencia inválida");
  }
  const state = job.state;
  if (
    state !== "queued" &&
    state !== "running" &&
    state !== "done" &&
    state !== "failed" &&
    state !== "cancelled"
  ) {
    throw new Error("El backend devolvió un estado de transferencia inválido");
  }
  return {
    id: readNumber(job.id, "id"),
    connectionId: readNumber(job.connectionId, "connectionId"),
    direction,
    remote: readString(job.remote, "remote"),
    local: readString(job.local, "local"),
    state,
    transferred: readNumber(job.transferred, "transferred"),
    total: readNullableNumber(job.total, "total"),
    error: readNullableString(job.error, "error"),
  };
}

function parseTransferList(value: unknown): TransferDto[] {
  if (!Array.isArray(value)) {
    throw new Error("El backend devolvió una lista de transferencias inválida");
  }
  return value.map(parseTransfer);
}

function parseEntry(value: unknown): EntryDto {
  const entry = asRecord(value);
  return {
    name: readString(entry.name, "name"),
    path: readString(entry.path, "path"),
    isDir: readBoolean(entry.isDir, "isDir"),
    size: readNumber(entry.size, "size"),
  };
}

function parseEntryList(value: unknown): EntryDto[] {
  if (!Array.isArray(value)) {
    throw new Error("El backend devolvió una lista de entradas inválida");
  }
  return value.map(parseEntry);
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

/** Lista las sesiones guardadas sin exponer sus contraseñas. */
export async function listSessions(): Promise<SessionDto[]> {
  return parseSessionList(await invoke<unknown>("list_sessions"));
}

/** Crea una sesión; la contraseña se envía solo cuando el usuario la tecleó. */
export async function createSession(input: SessionInput, password?: string): Promise<SessionDto> {
  return parseSessionDto(
    await invoke<unknown>("create_session", { input, password: password ?? null }),
  );
}

/** Actualiza una sesión e indica qué hacer con la contraseña guardada. */
export async function updateSession(
  id: number,
  input: SessionInput,
  password: PasswordInput,
): Promise<void> {
  await invoke("update_session", { id, input, password });
}

/** Duplica una sesión sin copiar secretos. */
export async function duplicateSession(id: number): Promise<SessionDto> {
  return parseSessionDto(await invoke<unknown>("duplicate_session", { id }));
}

/** Borra una sesión y su secreto asociado. */
export async function deleteSession(id: number): Promise<void> {
  await invoke("delete_session", { id });
}

/** Lista las carpetas del gestor de sesiones. */
export async function listFolders(): Promise<FolderDto[]> {
  return parseFolderList(await invoke<unknown>("list_folders"));
}

/** Inserta o actualiza una carpeta; sin id se asigna el siguiente disponible. */
export async function saveFolder(folder: FolderInput): Promise<FolderDto> {
  return parseFolderDto(
    await invoke<unknown>("save_folder", {
      id: folder.id ?? null,
      name: folder.name,
      parent: folder.parent ?? null,
    }),
  );
}

/** Borra una carpeta y aplica las reglas del repositorio a sus descendientes. */
export async function deleteFolder(id: number): Promise<void> {
  await invoke("delete_folder", { id });
}

/** Encola una descarga en un directorio local existente. */
export async function enqueueDownload(
  connectionId: number,
  remote: string,
  localDir: string,
): Promise<number> {
  return readNumber(
    await invoke<unknown>("enqueue_download", { connectionId, remote, localDir }),
    "id",
  );
}

/** Encola una subida de un archivo local al directorio remoto indicado. */
export async function enqueueUpload(
  connectionId: number,
  localPath: string,
  remoteDir: string,
): Promise<number> {
  return readNumber(
    await invoke<unknown>("enqueue_upload", { connectionId, localPath, remoteDir }),
    "id",
  );
}

/** Cancela un trabajo encolado o en marcha. */
export async function cancelTransfer(id: number): Promise<void> {
  await invoke("cancel_transfer", { id });
}

/** Elimina los trabajos terminados, fallidos o cancelados. */
export async function clearTransfers(): Promise<void> {
  await invoke("clear_transfers");
}

/** Devuelve el estado actual de la cola de transferencias. */
export async function listTransfers(): Promise<TransferDto[]> {
  return parseTransferList(await invoke<unknown>("list_transfers"));
}

/** Devuelve el directorio inicial de SFTP de la conexión indicada. */
export async function sftpHome(connectionId: number): Promise<string> {
  return readString(await invoke<unknown>("sftp_home", { connectionId }), "home");
}

/** Lista el directorio remoto indicado con carpetas primero y orden por nombre. */
export async function sftpList(connectionId: number, path: string): Promise<EntryDto[]> {
  return parseEntryList(await invoke<unknown>("sftp_list", { connectionId, path }));
}

/** Crea un directorio en la conexión indicada. */
export async function sftpMkdir(connectionId: number, path: string): Promise<void> {
  await invoke("sftp_mkdir", { connectionId, path });
}

/** Renombra una entrada remota. */
export async function sftpRename(connectionId: number, from: string, to: string): Promise<void> {
  await invoke("sftp_rename", { connectionId, from, to });
}

/** Elimina una entrada remota. */
export async function sftpRemove(connectionId: number, path: string): Promise<void> {
  await invoke("sftp_remove", { connectionId, path });
}

/** Suscribe la UI a los cambios de la cola, descartando cargas mal formadas. */
export async function onTransfers(listener: (jobs: TransferDto[]) => void): Promise<() => void> {
  const unlisten = await listen<unknown>("transfers", (event) => {
    try {
      listener(parseTransferList(event.payload));
    } catch {
      // Una carga inesperada se ignora sin romper la suscripción.
    }
  });
  return () => {
    unlisten();
  };
}

/** Suscribe la UI a los archivos arrastrados desde el sistema operativo. */
export async function onFileDrop(listener: (event: FileDropEvent) => void): Promise<() => void> {
  const unlisten = await getCurrentWebview().onDragDropEvent((event) => {
    const payload = event.payload;
    switch (payload.type) {
      case "enter":
      case "over":
        listener({ kind: "over", x: payload.position.x, y: payload.position.y });
        break;
      case "drop":
        listener({
          kind: "drop",
          paths: payload.paths,
          x: payload.position.x,
          y: payload.position.y,
        });
        break;
      case "leave":
        listener({ kind: "leave" });
        break;
    }
  });
  return () => {
    unlisten();
  };
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
