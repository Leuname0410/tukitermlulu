import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  type AuthArgs,
  type ConnectArgs,
  type ConnectResponse,
  type FileDropEvent,
  type FolderDto,
  type PasswordInput,
  type SessionDto,
  type SessionInput,
  type TransferDto,
  cancelTransfer,
  clearTransfers,
  closeTab,
  connect,
  createSession,
  deleteFolder,
  deleteSession,
  disconnect,
  downloadsDir,
  duplicateSession,
  enqueueDownload,
  enqueueUpload,
  exportDiagnostics,
  isCommandError,
  listFolders,
  listSessions,
  listTransfers,
  onFileDrop,
  onTransfers,
  saveFolder,
  sftpChmod,
  sftpHome,
  sftpList,
  sftpMkdir,
  sftpRemove,
  sftpRename,
  terminalResize,
  terminalWrite,
  updateSession,
} from "./api";

const { onDragDropEvent } = vi.hoisted(() => ({ onDragDropEvent: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => {
  class MockChannel {
    onmessage: ((message: unknown) => void) | undefined;

    constructor(onmessage?: (message: unknown) => void) {
      this.onmessage = onmessage;
    }
  }

  return { Channel: MockChannel, invoke: vi.fn() };
});

vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

vi.mock("@tauri-apps/api/webview", () => ({ getCurrentWebview: vi.fn() }));

interface ChannelLike {
  onmessage?: (message: unknown) => void;
}

interface ConnectPayload {
  tabId: number;
  args: ConnectArgs;
  onOutput: ChannelLike;
  onClosed: ChannelLike;
  onCwd: ChannelLike;
}

function connectPayload(call: Parameters<typeof invoke>): ConnectPayload {
  const payload = call[1];
  if (typeof payload !== "object" || payload === null || !("args" in payload)) {
    throw new Error("invoke de connect sin la forma esperada");
  }
  return payload as unknown as ConnectPayload;
}

function firstCall(): Parameters<typeof invoke> {
  const call = vi.mocked(invoke).mock.calls[0];
  if (!call) {
    throw new Error("connect no fue invocado");
  }
  return call;
}

const auth: AuthArgs = { kind: "password", remember: false };

function args(): ConnectArgs {
  return {
    host: "example.com",
    port: 22,
    username: "alice",
    auth,
    sessionId: 3,
    rows: 24,
    cols: 80,
  };
}

const sessionInput: SessionInput = {
  name: "prod",
  host: "example.com",
  port: 22,
  username: "alice",
  auth: { kind: "password", remember: true },
  folder: null,
  shellIntegration: true,
};

const sessionRaw = {
  id: 1,
  name: "prod",
  host: "example.com",
  port: 22,
  username: "alice",
  auth: { kind: "password", remember: true },
  folder: null,
  passwordSaved: true,
  shellIntegration: true,
};

const sessionDto: SessionDto = {
  ...sessionInput,
  folder: null,
  id: 1,
  passwordSaved: true,
  shellIntegration: true,
};

const folderRaw = { id: 2, name: "producción", parent: null };
const folderDto: FolderDto = folderRaw;

const transferRaw = {
  id: 5,
  connectionId: 1,
  direction: "download",
  remote: "/srv/a.txt",
  local: "downloads/a.txt",
  state: "running",
  transferred: 4,
  total: 10,
  error: null,
};

const transferDto: TransferDto = {
  id: 5,
  connectionId: 1,
  direction: "download",
  remote: "/srv/a.txt",
  local: "downloads/a.txt",
  state: "running",
  transferred: 4,
  total: 10,
  error: null,
};

describe("typed terminal api", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    vi.mocked(listen).mockReset();
    vi.mocked(getCurrentWebview).mockReset();
    onDragDropEvent.mockReset();
    vi.mocked(getCurrentWebview).mockReturnValue({
      onDragDropEvent,
    } as unknown as ReturnType<typeof getCurrentWebview>);
  });

  it("should invoke connect with three channels and return typed ids when backend connects", async () => {
    vi.mocked(invoke).mockResolvedValueOnce({
      kind: "connected",
      connection_id: 1,
      channel_id: 2,
    });
    const onData = vi.fn<(bytes: Uint8Array) => void>();
    const onClosed = vi.fn<(status: number | null) => void>();
    const request = args();

    const response = await connect(7, request, { onData, onClosed });

    const call = firstCall();
    expect(call[0]).toBe("connect");
    const payload = connectPayload(call);
    expect(payload.tabId).toBe(7);
    expect(payload.args).toBe(request);
    expect(payload.onOutput).not.toBe(payload.onClosed);
    expect(payload.onCwd).not.toBe(payload.onOutput);
    expect(payload.onCwd).not.toBe(payload.onClosed);
    expect(response).toEqual({ kind: "connected", connectionId: 1, channelId: 2 });
  });

  it("should deliver output bytes and close status when channel messages arrive", async () => {
    vi.mocked(invoke).mockResolvedValueOnce({
      kind: "connected",
      connection_id: 1,
      channel_id: 2,
    });
    const onData = vi.fn<(bytes: Uint8Array) => void>();
    const onClosed = vi.fn<(status: number | null) => void>();

    await connect(7, args(), { onData, onClosed });

    const payload = connectPayload(firstCall());
    const bytes = new Uint8Array([1, 2, 3]);
    payload.onOutput.onmessage?.(bytes.buffer);
    expect(onData).toHaveBeenCalledTimes(1);
    expect(onData.mock.calls[0]?.[0]).toEqual(bytes);

    payload.onClosed.onmessage?.({ exit_status: 7 });
    expect(onClosed).toHaveBeenCalledWith(7);
    payload.onClosed.onmessage?.({ exit_status: null });
    expect(onClosed).toHaveBeenLastCalledWith(null);
  });

  it("should deliver cwd changes and discard malformed messages when channel messages arrive", async () => {
    vi.mocked(invoke).mockResolvedValueOnce({
      kind: "connected",
      connection_id: 1,
      channel_id: 2,
    });
    const onCwd = vi.fn<(path: string) => void>();

    await connect(7, args(), { onData: () => {}, onClosed: () => {}, onCwd });

    const { onCwd: channel } = connectPayload(firstCall());
    channel.onmessage?.({ path: "/tmp" });
    expect(onCwd).toHaveBeenCalledTimes(1);
    expect(onCwd).toHaveBeenCalledWith("/tmp");

    for (const malformed of [null, {}, { path: 7 }, { path: null }, ""]) {
      channel.onmessage?.(malformed);
    }
    expect(onCwd).toHaveBeenCalledTimes(1);
  });

  it("should ignore cwd messages when no handler is provided", async () => {
    vi.mocked(invoke).mockResolvedValueOnce({
      kind: "connected",
      connection_id: 1,
      channel_id: 2,
    });

    await connect(7, args(), { onData: () => {}, onClosed: () => {} });

    connectPayload(firstCall()).onCwd.onmessage?.({ path: "/tmp" });
  });

  it("should map approval and prompts when backend requests them", async () => {
    const cases: Array<[unknown, ConnectResponse]> = [
      [
        { kind: "needs_host_key_approval", fingerprint: "SHA256:abc" },
        { kind: "needs_host_key_approval", fingerprint: "SHA256:abc" },
      ],
      [{ kind: "needs_password" }, { kind: "needs_password" }],
      [{ kind: "needs_passphrase" }, { kind: "needs_passphrase" }],
    ];

    for (const [raw, expected] of cases) {
      vi.mocked(invoke).mockResolvedValueOnce(raw);
      await expect(
        connect(7, args(), { onData: () => {}, onClosed: () => {} }),
      ).resolves.toEqual(expected);
    }
  });

  it("should send raw bytes with the channel header when writing terminal", async () => {
    vi.mocked(invoke).mockResolvedValueOnce(undefined);
    const data = new Uint8Array([65, 66]);

    await terminalWrite(7, data);

    expect(vi.mocked(invoke)).toHaveBeenCalledWith("terminal_write", data, {
      headers: { "x-channel-id": "7" },
    });
  });

  it("should invoke resize and disconnect with camelCase arguments", async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);

    await terminalResize(7, 24, 80);
    await disconnect(9);

    expect(vi.mocked(invoke)).toHaveBeenNthCalledWith(1, "terminal_resize", {
      channelId: 7,
      rows: 24,
      cols: 80,
    });
    expect(vi.mocked(invoke)).toHaveBeenNthCalledWith(2, "disconnect", { connectionId: 9 });
  });

  it("should invoke close_tab with the tab id", async () => {
    vi.mocked(invoke).mockResolvedValueOnce(undefined);

    await closeTab(11);

    expect(vi.mocked(invoke)).toHaveBeenCalledWith("close_tab", { tabId: 11 });
  });

  it("should narrow only well-formed command errors", () => {
    const valid = { kind: "auth_rejected", message: "Credenciales rechazadas" };

    expect(isCommandError(valid)).toBe(true);
    expect(isCommandError({ ...valid, fingerprint: "SHA256:abc" })).toBe(true);
    expect(isCommandError({ ...valid, fingerprint: null })).toBe(true);
    for (const invalid of [
      null,
      "boom",
      { message: "sin kind" },
      { kind: 7, message: "kind inválido" },
      { kind: "io", message: 3 },
      { kind: "io", message: "x", fingerprint: 7 },
    ]) {
      expect(isCommandError(invalid)).toBe(false);
    }
  });

  it("should parse shell integration when listing sessions", async () => {
    vi.mocked(invoke).mockResolvedValueOnce([{ ...sessionRaw, shellIntegration: false }]);

    const [parsed] = await listSessions();

    expect(parsed?.shellIntegration).toBe(false);
  });

  it("should invoke session commands with camelCase arguments when managing sessions", async () => {
    const password: PasswordInput = { kind: "set", value: "new-pw" };
    vi.mocked(invoke).mockResolvedValue(sessionRaw);
    expect(await createSession(sessionInput, "pw")).toEqual(sessionDto);
    expect(vi.mocked(invoke)).toHaveBeenNthCalledWith(1, "create_session", {
      input: sessionInput,
      password: "pw",
    });

    vi.mocked(invoke).mockResolvedValue(undefined);
    await updateSession(1, sessionInput, password);
    expect(vi.mocked(invoke)).toHaveBeenNthCalledWith(2, "update_session", {
      id: 1,
      input: sessionInput,
      password,
    });

    vi.mocked(invoke).mockResolvedValue(sessionRaw);
    expect(await duplicateSession(1)).toEqual(sessionDto);
    expect(vi.mocked(invoke)).toHaveBeenNthCalledWith(3, "duplicate_session", { id: 1 });

    vi.mocked(invoke).mockResolvedValue(undefined);
    await deleteSession(1);
    expect(vi.mocked(invoke)).toHaveBeenNthCalledWith(4, "delete_session", { id: 1 });
  });

  it("should invoke folder commands with null optionals when saving and deleting", async () => {
    vi.mocked(invoke).mockResolvedValue(folderRaw);
    expect(await saveFolder({ name: "producción" })).toEqual(folderDto);
    expect(vi.mocked(invoke)).toHaveBeenNthCalledWith(1, "save_folder", {
      id: null,
      name: "producción",
      parent: null,
    });

    vi.mocked(invoke).mockResolvedValue([folderRaw]);
    expect(await listFolders()).toEqual([folderDto]);
    expect(vi.mocked(invoke)).toHaveBeenNthCalledWith(2, "list_folders");

    vi.mocked(invoke).mockResolvedValue(undefined);
    await deleteFolder(2);
    expect(vi.mocked(invoke)).toHaveBeenNthCalledWith(3, "delete_folder", { id: 2 });
  });

  it("should return parsed sessions when listing them", async () => {
    vi.mocked(invoke).mockResolvedValue([sessionRaw]);

    expect(await listSessions()).toEqual([sessionDto]);
    expect(vi.mocked(invoke)).toHaveBeenCalledWith("list_sessions");
  });

  it("should reject malformed responses when listing sessions", async () => {
    const malformed: unknown[] = [
      null,
      {},
      [{ ...sessionRaw, id: "1" }],
      [{ ...sessionRaw, passwordSaved: "true" }],
      [{ ...sessionRaw, shellIntegration: "true" }],
      [{ ...sessionRaw, auth: { kind: "key", path: 7, hasPassphrase: false } }],
    ];
    for (const raw of malformed) {
      vi.mocked(invoke).mockResolvedValueOnce(raw);
      await expect(listSessions()).rejects.toThrow();
    }
  });

  it("should invoke transfer commands with camelCase arguments when managing transfers", async () => {
    vi.mocked(invoke).mockResolvedValueOnce(7);
    expect(await enqueueDownload(3, "/srv/a.txt", "C:/downloads")).toBe(7);
    expect(vi.mocked(invoke)).toHaveBeenNthCalledWith(1, "enqueue_download", {
      connectionId: 3,
      remote: "/srv/a.txt",
      localDir: "C:/downloads",
    });

    vi.mocked(invoke).mockResolvedValueOnce(8);
    expect(await enqueueUpload(4, "uploads/a.txt", "/srv")).toBe(8);
    expect(vi.mocked(invoke)).toHaveBeenNthCalledWith(2, "enqueue_upload", {
      connectionId: 4,
      localPath: "uploads/a.txt",
      remoteDir: "/srv",
    });

    vi.mocked(invoke).mockResolvedValue(undefined);
    await cancelTransfer(7);
    expect(vi.mocked(invoke)).toHaveBeenNthCalledWith(3, "cancel_transfer", { id: 7 });
    await clearTransfers();
    expect(vi.mocked(invoke)).toHaveBeenNthCalledWith(4, "clear_transfers");
  });

  it("should return parsed transfers when listing them", async () => {
    vi.mocked(invoke).mockResolvedValueOnce([transferRaw]);

    expect(await listTransfers()).toEqual([transferDto]);
    expect(vi.mocked(invoke)).toHaveBeenCalledWith("list_transfers");
  });

  it("should invoke sftp commands with camelCase arguments", async () => {
    vi.mocked(invoke).mockResolvedValue("/home/alice");
    expect(await sftpHome(3)).toBe("/home/alice");
    expect(vi.mocked(invoke)).toHaveBeenNthCalledWith(1, "sftp_home", { connectionId: 3 });

    const entry = {
      name: "a.txt",
      path: "/home/alice/a.txt",
      isDir: false,
      size: 5,
      permissions: 0o640,
    };
    vi.mocked(invoke).mockResolvedValue([entry]);
    expect(await sftpList(3, "/home/alice")).toEqual([entry]);
    expect(vi.mocked(invoke)).toHaveBeenNthCalledWith(2, "sftp_list", {
      connectionId: 3,
      path: "/home/alice",
    });

    vi.mocked(invoke).mockResolvedValue(undefined);
    await sftpMkdir(3, "/home/alice/new");
    expect(vi.mocked(invoke)).toHaveBeenNthCalledWith(3, "sftp_mkdir", {
      connectionId: 3,
      path: "/home/alice/new",
    });
    await sftpRename(3, "/home/alice/a", "/home/alice/b");
    expect(vi.mocked(invoke)).toHaveBeenNthCalledWith(4, "sftp_rename", {
      connectionId: 3,
      from: "/home/alice/a",
      to: "/home/alice/b",
    });
    await sftpRemove(3, "/home/alice/b");
    expect(vi.mocked(invoke)).toHaveBeenNthCalledWith(5, "sftp_remove", {
      connectionId: 3,
      path: "/home/alice/b",
    });

    vi.mocked(invoke).mockResolvedValue(undefined);
    await sftpChmod(3, "/home/alice/b", 0o640);
    expect(vi.mocked(invoke)).toHaveBeenNthCalledWith(6, "sftp_chmod", {
      connectionId: 3,
      path: "/home/alice/b",
      mode: 416,
    });
  });

  it("should invoke downloads_dir without arguments when reading the downloads folder", async () => {
    vi.mocked(invoke).mockResolvedValueOnce("C:/Users/alice/Downloads");

    expect(await downloadsDir()).toBe("C:/Users/alice/Downloads");
    expect(vi.mocked(invoke)).toHaveBeenCalledWith("downloads_dir");
  });

  it("should invoke export_diagnostics and return the path when exporting", async () => {
    const path = "C:/Users/alice/Downloads/tukitermlulu-diagnostico-20260101-010101.txt";
    vi.mocked(invoke).mockResolvedValueOnce(path);

    expect(await exportDiagnostics()).toBe(path);
    expect(vi.mocked(invoke)).toHaveBeenCalledWith("export_diagnostics");
  });

  it("should reject malformed diagnostic responses when exporting", async () => {
    for (const raw of [null, 7, { path: "C:/Downloads/diagnostico.txt" }]) {
      vi.mocked(invoke).mockResolvedValueOnce(raw);
      await expect(exportDiagnostics()).rejects.toThrow();
    }
  });

  it("should reject malformed sftp responses when listing and reading home", async () => {
    const valid = { name: "a", path: "/a", isDir: true, size: 0 };
    for (const raw of [
      null,
      {},
      [null],
      [{ ...valid, name: 3 }],
      [{ ...valid, path: null }],
      [{ ...valid, isDir: "true" }],
      [{ ...valid, size: "0" }],
      [{ ...valid, permissions: "640" }],
      { ...valid },
    ]) {
      vi.mocked(invoke).mockResolvedValueOnce(raw);
      await expect(sftpList(1, "/")).rejects.toThrow();
    }
    for (const raw of [null, 7, { path: "/home/alice" }]) {
      vi.mocked(invoke).mockResolvedValueOnce(raw);
      await expect(sftpHome(1)).rejects.toThrow();
    }
  });

  it("should deliver only well-formed transfer events when listening", async () => {
    const unlisten = vi.fn();
    vi.mocked(listen).mockResolvedValueOnce(unlisten);
    const listener = vi.fn<(jobs: TransferDto[]) => void>();

    const unsubscribe = await onTransfers(listener);

    expect(vi.mocked(listen)).toHaveBeenCalledWith("transfers", expect.any(Function));
    const handler = vi.mocked(listen).mock.calls[0]?.[1] as unknown as (event: {
      payload: unknown;
    }) => void;
    handler({ payload: [transferRaw] });
    expect(listener).toHaveBeenCalledWith([transferDto]);

    handler({ payload: { ...transferRaw, total: "diez" } });
    handler({ payload: { ...transferRaw, state: "unknown" } });
    handler({ payload: { ...transferRaw, direction: "sideways" } });
    handler({ payload: null });
    expect(listener).toHaveBeenCalledTimes(1);

    unsubscribe();
    expect(unlisten).toHaveBeenCalledTimes(1);
  });

  it("should normalize OS drag drop events when subscribing to file drops", async () => {
    const unlisten = vi.fn();
    onDragDropEvent.mockResolvedValueOnce(unlisten);
    const listener = vi.fn<(event: FileDropEvent) => void>();

    const unsubscribe = await onFileDrop(listener);

    expect(getCurrentWebview).toHaveBeenCalledTimes(1);
    const call = onDragDropEvent.mock.calls[0];
    if (!call) {
      throw new Error("onDragDropEvent no fue invocado");
    }
    const handler = call[0] as (event: { payload: unknown }) => void;
    handler({ payload: { type: "enter", paths: ["a.txt"], position: { x: 10, y: 20 } } });
    handler({ payload: { type: "over", position: { x: 11, y: 21 } } });
    handler({
      payload: { type: "drop", paths: ["a.txt", "b.txt"], position: { x: 12, y: 22 } },
    });
    handler({ payload: { type: "leave" } });
    expect(listener.mock.calls.map(([event]) => event)).toEqual([
      { kind: "over", x: 10, y: 20 },
      { kind: "over", x: 11, y: 21 },
      { kind: "drop", paths: ["a.txt", "b.txt"], x: 12, y: 22 },
      { kind: "leave" },
    ]);

    unsubscribe();
    expect(unlisten).toHaveBeenCalledTimes(1);
  });
});
