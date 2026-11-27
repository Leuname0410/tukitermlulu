import type { CommandError, ConnectResponse, TerminalHandlers } from "../lib/api";
import { closeTab, connect, disconnect } from "../lib/api";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { type ConnectTarget, type Tab, type TabAction, tabsReducer } from "./tabState";
import { createTabController } from "./tabController";

vi.mock("../lib/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../lib/api")>();
  return { ...actual, connect: vi.fn(), disconnect: vi.fn(), closeTab: vi.fn() };
});

const size = { rows: 24, cols: 80 };
const target: ConnectTarget = {
  host: "example.com",
  port: 22,
  username: "alice",
  auth: { kind: "password", remember: false },
};

function tab(overrides: Partial<Tab> = {}): Tab {
  return {
    id: 7,
    title: "alice@example.com",
    target,
    status: { kind: "connecting" },
    ...overrides,
  };
}

function handlersFromCall(): TerminalHandlers {
  const handlers = vi.mocked(connect).mock.calls[0]?.[2];
  if (!handlers) {
    throw new Error("connect no fue invocado con handlers");
  }
  return handlers;
}

function harness() {
  const actions: TabAction[] = [];
  let state: Tab[] = [tab()];
  const dispatch = (action: TabAction): void => {
    actions.push(action);
    state = tabsReducer(state, action);
  };
  return {
    actions,
    dispatch,
    state: (): Tab[] => state,
    controller: createTabController(tab(), dispatch),
  };
}

describe("tabsReducer", () => {
  it("should move a tab from connecting to connected with the backend ids", () => {
    const initial = [tab()];

    const next = tabsReducer(initial, {
      type: "resolved",
      id: 7,
      response: { kind: "connected", connectionId: 3, channelId: 4 },
    });

    expect(next[0]?.status).toEqual({ kind: "connected", connectionId: 3, channelId: 4 });
    expect(initial[0]?.status).toEqual({ kind: "connecting" });
  });

  it("should map approval and prompt responses into tab statuses", () => {
    const responses: Array<[ConnectResponse, Tab["status"]]> = [
      [{ kind: "needs_password" }, { kind: "needs_password" }],
      [{ kind: "needs_passphrase" }, { kind: "needs_passphrase" }],
      [
        { kind: "needs_host_key_approval", fingerprint: "SHA256:abc" },
        { kind: "needs_host_key_approval", fingerprint: "SHA256:abc" },
      ],
    ];
    for (const [response, expected] of responses) {
      const next = tabsReducer([tab()], { type: "resolved", id: 7, response });
      expect(next[0]?.status).toEqual(expected);
    }
  });

  it("should set the error status when connecting fails", () => {
    const error: CommandError = { kind: "auth_rejected", message: "Credenciales rechazadas" };

    const next = tabsReducer([tab()], { type: "failed", id: 7, error });

    expect(next[0]?.status).toEqual({ kind: "error", error });
  });

  it("should mark a tab as dropped with its exit status when the remote closes", () => {
    const next = tabsReducer([tab()], { type: "remote_closed", id: 7, exitStatus: 9 });

    expect(next[0]?.status).toEqual({ kind: "dropped", exitStatus: 9 });
  });

  it("should mark a tab as dropped with a null exit status when the remote closes", () => {
    const next = tabsReducer([tab()], { type: "remote_closed", id: 7, exitStatus: null });

    expect(next[0]?.status).toEqual({ kind: "dropped", exitStatus: null });
  });

  it("should remove the tab when it is closed", () => {
    expect(tabsReducer([tab(), tab({ id: 8 })], { type: "closed", id: 7 })).toEqual([
      tab({ id: 8 }),
    ]);
  });

  it("should keep the same tabs when an action targets an unknown id", () => {
    const tabs = [tab()];
    const actions: TabAction[] = [
      { type: "connecting", id: 99 },
      { type: "resolved", id: 99, response: { kind: "needs_password" } },
      { type: "failed", id: 99, error: { kind: "io", message: "boom" } },
      { type: "remote_closed", id: 99, exitStatus: 1 },
      { type: "closed", id: 99 },
    ];
    for (const action of actions) {
      expect(tabsReducer(tabs, action)).toBe(tabs);
    }
  });
});

describe("createTabController", () => {
  beforeEach(() => {
    vi.mocked(connect).mockReset();
    vi.mocked(disconnect).mockReset();
    vi.mocked(closeTab).mockReset();
    vi.mocked(connect).mockResolvedValue({ kind: "connected", connectionId: 3, channelId: 4 });
    vi.mocked(disconnect).mockResolvedValue(undefined);
    vi.mocked(closeTab).mockResolvedValue(undefined);
  });

  it("should dispatch connecting then resolved with the backend ids when connect succeeds", async () => {
    const h = harness();

    await h.controller.connect({ password: "s3cret" }, size);

    expect(h.actions).toEqual([
      { type: "connecting", id: 7 },
      { type: "resolved", id: 7, response: { kind: "connected", connectionId: 3, channelId: 4 } },
    ]);
    expect(vi.mocked(connect)).toHaveBeenCalledWith(
      7,
      { ...target, rows: 24, cols: 80, password: "s3cret" },
      expect.objectContaining({ onData: expect.any(Function), onClosed: expect.any(Function) }),
    );
  });

  it("should dispatch failed with the command error when connect is rejected", async () => {
    const error: CommandError = { kind: "auth_rejected", message: "Credenciales rechazadas" };
    vi.mocked(connect).mockRejectedValueOnce(error);
    const h = harness();

    await h.controller.connect({}, size);

    expect(h.actions).toEqual([
      { type: "connecting", id: 7 },
      { type: "failed", id: 7, error },
    ]);
  });

  it("should dispatch failed with an io error when the rejection is not a command error", async () => {
    vi.mocked(connect).mockRejectedValueOnce("boom");
    const h = harness();

    await h.controller.connect({}, size);

    expect(h.actions[1]).toEqual({
      type: "failed",
      id: 7,
      error: { kind: "io", message: "Error inesperado" },
    });
  });

  it("should dispatch remote_closed when the backend closes the session", async () => {
    const h = harness();
    await h.controller.connect({}, size);
    const handlers = handlersFromCall();

    handlers.onClosed(9);
    handlers.onClosed(null);

    expect(h.actions).toContainEqual({ type: "remote_closed", id: 7, exitStatus: 9 });
    expect(h.actions).toContainEqual({ type: "remote_closed", id: 7, exitStatus: null });
  });

  it("should flush buffered bytes on attach, stream afterwards and stop on detach", async () => {
    const h = harness();
    await h.controller.connect({}, size);
    const handlers = handlersFromCall();
    const received: number[][] = [];

    handlers.onData(new Uint8Array([1, 2]));
    handlers.onData(new Uint8Array([3]));
    const detach = h.controller.attach((bytes) => {
      received.push(Array.from(bytes));
    });

    expect(received).toEqual([[1, 2], [3]]);
    handlers.onData(new Uint8Array([4]));
    expect(received).toEqual([[1, 2], [3], [4]]);
    detach();
    handlers.onData(new Uint8Array([5]));
    expect(received).toEqual([[1, 2], [3], [4]]);
  });

  it("should discard the oldest bytes when more than 1 MiB arrives before attach", async () => {
    const h = harness();
    await h.controller.connect({}, size);
    const handlers = handlersFromCall();
    const cap = 1024 * 1024;
    handlers.onData(new Uint8Array(cap + 10).fill(65));
    handlers.onData(new Uint8Array([66]));

    let count = 0;
    let last = -1;
    h.controller.attach((bytes) => {
      count += bytes.byteLength;
      last = bytes[bytes.byteLength - 1] ?? -1;
    });

    expect(count).toBe(cap);
    expect(last).toBe(66);
  });

  it("should reconnect without password or passphrase", async () => {
    const h = harness();
    await h.controller.connect({ password: "s3cret", passphrase: "p4ss" }, size);

    await h.controller.reconnect(size);

    const args = vi.mocked(connect).mock.calls[1]?.[1];
    expect(args).toEqual({ ...target, rows: 24, cols: 80 });
    expect(args).not.toHaveProperty("password");
    expect(args).not.toHaveProperty("passphrase");
    expect(h.actions.filter((action) => action.type === "connecting")).toHaveLength(2);
  });

  it("should disconnect and close the tab even when disconnect fails", async () => {
    const h = harness();
    await h.controller.connect({}, size);
    vi.mocked(disconnect).mockRejectedValueOnce(new Error("desconexión fallida"));

    await h.controller.close();

    expect(disconnect).toHaveBeenCalledWith(3);
    expect(closeTab).toHaveBeenCalledWith(7);
    expect(h.actions.at(-1)).toEqual({ type: "closed", id: 7 });
  });

  it("should close the tab without disconnecting when there is no connection", async () => {
    const h = harness();

    await h.controller.close();

    expect(disconnect).not.toHaveBeenCalled();
    expect(closeTab).toHaveBeenCalledWith(7);
    expect(h.actions.at(-1)).toEqual({ type: "closed", id: 7 });
  });

  it("should never store secrets in the reducer state when connecting", async () => {
    const password = "contraseña-muy-secreta";
    const passphrase = "passphrase-muy-secreta";
    const h = harness();

    await h.controller.connect({ password, passphrase }, size);

    const snapshot = JSON.stringify(h.state());
    expect(snapshot).not.toContain(password);
    expect(snapshot).not.toContain(passphrase);
  });
});
