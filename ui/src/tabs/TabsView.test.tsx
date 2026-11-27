import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "../App";
import type { CommandError, ConnectArgs, ConnectResponse, TerminalHandlers } from "../lib/api";
import { closeTab, connect, disconnect } from "../lib/api";

vi.mock("../lib/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../lib/api")>();
  return {
    ...actual,
    connect: vi.fn(),
    disconnect: vi.fn(),
    closeTab: vi.fn(),
    terminalWrite: vi.fn(),
    terminalResize: vi.fn(),
  };
});

vi.mock("../terminal/TerminalView", () => ({
  TerminalView: (props: { channelId: number }) => (
    <div data-testid={`terminal-${props.channelId}`}>terminal:{props.channelId}</div>
  ),
}));

vi.mock("../sftp/SftpSidebar", () => ({
  SftpSidebar: (props: { connectionId: number }) => (
    <aside aria-label="Explorador SFTP">sidebar:{props.connectionId}</aside>
  ),
}));

function fillQuickConnect(overrides: Partial<{ host: string; port: string; username: string }> = {}): void {
  fireEvent.change(screen.getByLabelText("Host"), {
    target: { value: overrides.host ?? "example.com" },
  });
  fireEvent.change(screen.getByLabelText("Puerto"), {
    target: { value: overrides.port ?? "22" },
  });
  fireEvent.change(screen.getByLabelText("Usuario"), {
    target: { value: overrides.username ?? "alice" },
  });
}

function submitQuickConnect(): void {
  fireEvent.click(screen.getByRole("button", { name: "Conectar" }));
}

async function openConnectingTab(): Promise<void> {
  fillQuickConnect();
  submitQuickConnect();
  await screen.findByRole("tab", { name: /alice@example.com/i });
}

function connectCall(index = 0): [number, ConnectArgs, TerminalHandlers] {
  const call = vi.mocked(connect).mock.calls[index];
  if (!call) {
    throw new Error(`connect call ${index} missing`);
  }
  return call;
}

function deferred<T>(): {
  promise: Promise<T>;
  resolve: (value: T) => void;
  reject: (reason?: unknown) => void;
} {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

describe("TabsView and QuickConnect", () => {
  beforeEach(() => {
    vi.mocked(connect).mockReset();
    vi.mocked(disconnect).mockReset();
    vi.mocked(closeTab).mockReset();
    vi.mocked(disconnect).mockResolvedValue(undefined);
    vi.mocked(closeTab).mockResolvedValue(undefined);
  });

  afterEach(() => {
    cleanup();
  });

  it("should open a connecting tab and call api.connect with target and tabId when submitting form", async () => {
    const pending = deferred<ConnectResponse>();
    vi.mocked(connect).mockReturnValueOnce(pending.promise);

    render(<App />);
    fillQuickConnect();
    submitQuickConnect();

    const tab = await screen.findByRole("tab", { name: /alice@example.com/i });
    expect(tab.textContent).toContain("Conectando…");
    expect(vi.mocked(connect)).toHaveBeenCalledTimes(1);
    const [tabId, args] = connectCall(0);
    expect(tabId).toBe(1);
    expect(args).toMatchObject({
      host: "example.com",
      port: 22,
      username: "alice",
      auth: { kind: "password", remember: false },
      rows: 24,
      cols: 80,
    });
  });

  it("should mount TerminalView with channelId when response is connected", async () => {
    vi.mocked(connect).mockResolvedValueOnce({
      kind: "connected",
      connectionId: 10,
      channelId: 20,
    });

    render(<App />);
    await openConnectingTab();

    expect(await screen.findByTestId("terminal-20")).toBeTruthy();
    expect(screen.getByText("Conectado")).toBeTruthy();
  });

  it("should show passphrase field and send passphrase then clear field when needs_passphrase", async () => {
    vi.mocked(connect)
      .mockResolvedValueOnce({ kind: "needs_passphrase" })
      .mockResolvedValueOnce({ kind: "connected", connectionId: 11, channelId: 21 });

    render(<App />);
    await openConnectingTab();
    await screen.findByLabelText("Passphrase");

    const input = screen.getByLabelText("Passphrase") as HTMLInputElement;
    fireEvent.change(input, { target: { value: "secret-pass" } });
    fireEvent.click(within(screen.getByRole("tabpanel")).getByRole("button", { name: "Conectar" }));

    await screen.findByTestId("terminal-21");
    const [, args] = connectCall(1);
    expect(args.passphrase).toBe("secret-pass");
    expect(screen.queryByLabelText("Passphrase")).toBeNull();
  });

  it("should retry with approvedFingerprint on accept and not offer accept for host_key_changed", async () => {
    vi.mocked(connect)
      .mockResolvedValueOnce({ kind: "needs_host_key_approval", fingerprint: "SHA256:abc" })
      .mockResolvedValueOnce({ kind: "connected", connectionId: 12, channelId: 22 });

    render(<App />);
    await openConnectingTab();
    expect(await screen.findByText("SHA256:abc")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Aceptar" }));

    await screen.findByTestId("terminal-22");
    const [, args] = connectCall(1);
    expect(args.approvedFingerprint).toBe("SHA256:abc");

    cleanup();
    vi.mocked(connect).mockReset();
    vi.mocked(disconnect).mockReset();
    vi.mocked(closeTab).mockReset();
    vi.mocked(disconnect).mockResolvedValue(undefined);
    vi.mocked(closeTab).mockResolvedValue(undefined);

    const error: CommandError = {
      kind: "host_key_changed",
      message: "La huella del host cambió",
      fingerprint: "SHA256:bad",
    };
    vi.mocked(connect).mockRejectedValueOnce(error);

    render(<App />);
    fillQuickConnect();
    submitQuickConnect();
    await screen.findByRole("tab", { name: /alice@example.com/i });
    expect(await screen.findByText("La huella del host cambió")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Aceptar" })).toBeNull();
  });

  it("should show dropped session on onClosed and reconnect without passphrase", async () => {
    vi.mocked(connect)
      .mockResolvedValueOnce({ kind: "connected", connectionId: 13, channelId: 23 })
      .mockResolvedValueOnce({ kind: "connected", connectionId: 14, channelId: 24 });

    render(<App />);
    await openConnectingTab();
    await screen.findByTestId("terminal-23");

    const handlers = connectCall(0)[2];
    handlers.onClosed(0);

    expect(await screen.findByText("Sesión terminada (código 0)")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Reconectar" }));

    await screen.findByTestId("terminal-24");
    const [, args] = connectCall(1);
    expect(args).not.toHaveProperty("passphrase");
    expect(args).not.toHaveProperty("password");
  });

  it("should close tab calling disconnect and closeTab and removing it", async () => {
    vi.mocked(connect).mockResolvedValueOnce({
      kind: "connected",
      connectionId: 15,
      channelId: 25,
    });

    render(<App />);
    await openConnectingTab();
    await screen.findByTestId("terminal-25");

    fireEvent.click(screen.getByRole("button", { name: "Cerrar alice@example.com" }));

    await waitFor(() => {
      expect(disconnect).toHaveBeenCalledWith(15);
      expect(closeTab).toHaveBeenCalledWith(1);
    });
    await waitFor(() => {
      expect(screen.queryByRole("tab", { name: /alice@example.com/i })).toBeNull();
    });
  });

  it("should keep inactive terminal mounted but hidden when switching tabs", async () => {
    vi.mocked(connect)
      .mockResolvedValueOnce({ kind: "connected", connectionId: 16, channelId: 26 })
      .mockResolvedValueOnce({ kind: "connected", connectionId: 17, channelId: 27 });

    render(<App />);
    fillQuickConnect({ host: "one.example.com", username: "alice" });
    submitQuickConnect();
    await screen.findByTestId("terminal-26");

    fillQuickConnect({ host: "two.example.com", username: "bob" });
    submitQuickConnect();
    await screen.findByTestId("terminal-27");

    const tabAlice = screen.getByRole("tab", { name: /alice@one.example.com/i });
    fireEvent.click(tabAlice);

    expect(screen.getByTestId("terminal-26")).toBeTruthy();
    expect(screen.getByTestId("terminal-27")).toBeTruthy();
    const panel27 = screen.getByTestId("terminal-27").closest("[role='tabpanel']");
    expect(panel27?.hasAttribute("hidden")).toBe(true);
  });

  it("should show the terminal and the sftp sidebar only in the connected tab", async () => {
    const pending = deferred<ConnectResponse>();
    vi.mocked(connect)
      .mockResolvedValueOnce({ kind: "connected", connectionId: 30, channelId: 30 })
      .mockReturnValueOnce(pending.promise);

    render(<App />);
    fillQuickConnect({ host: "one.example.com", username: "alice" });
    submitQuickConnect();
    await screen.findByTestId("terminal-30");

    fillQuickConnect({ host: "two.example.com", username: "bob" });
    submitQuickConnect();
    await screen.findByRole("tab", { name: /bob@two.example.com/i });

    const panelBob = screen.getByRole("tabpanel");
    expect(within(panelBob).queryByRole("region", { name: "Terminal" })).toBeNull();
    expect(within(panelBob).queryByLabelText("Explorador SFTP")).toBeNull();

    fireEvent.click(screen.getByRole("tab", { name: /alice@one.example.com/i }));
    const panelAlice = screen.getByRole("tabpanel");
    expect(within(panelAlice).getByRole("region", { name: "Terminal" })).toBeTruthy();
    expect(within(panelAlice).getByLabelText("Explorador SFTP")).toBeTruthy();
  });
});
