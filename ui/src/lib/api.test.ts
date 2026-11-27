import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  type AuthArgs,
  type ConnectArgs,
  type ConnectResponse,
  type FolderDto,
  type PasswordInput,
  type SessionDto,
  type SessionInput,
  closeTab,
  connect,
  createSession,
  deleteFolder,
  deleteSession,
  disconnect,
  duplicateSession,
  isCommandError,
  listFolders,
  listSessions,
  saveFolder,
  terminalResize,
  terminalWrite,
  updateSession,
} from "./api";

vi.mock("@tauri-apps/api/core", () => {
  class MockChannel {
    onmessage: ((message: unknown) => void) | undefined;

    constructor(onmessage?: (message: unknown) => void) {
      this.onmessage = onmessage;
    }
  }

  return { Channel: MockChannel, invoke: vi.fn() };
});

interface ChannelLike {
  onmessage?: (message: unknown) => void;
}

interface ConnectPayload {
  tabId: number;
  args: ConnectArgs;
  onOutput: ChannelLike;
  onClosed: ChannelLike;
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
};

const sessionDto: SessionDto = { ...sessionInput, id: 1, passwordSaved: true };

const folderRaw = { id: 2, name: "producción", parent: null };
const folderDto: FolderDto = folderRaw;

describe("typed terminal api", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
  });

  it("should invoke connect with two channels and return typed ids when backend connects", async () => {
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
      [{ ...sessionRaw, auth: { kind: "key", path: 7, hasPassphrase: false } }],
    ];
    for (const raw of malformed) {
      vi.mocked(invoke).mockResolvedValueOnce(raw);
      await expect(listSessions()).rejects.toThrow();
    }
  });
});
