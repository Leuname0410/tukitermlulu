import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "../App";
import type { CommandError, SessionDto } from "../lib/api";
import { closeTab, connect, disconnect, listFolders, listSessions, updateSession } from "../lib/api";
import { HostKeyDialog } from "./HostKeyDialog";
import { SecretDialog } from "./SecretDialog";

vi.mock("../lib/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../lib/api")>();
  return {
    ...actual,
    connect: vi.fn(),
    disconnect: vi.fn(),
    closeTab: vi.fn(),
    terminalWrite: vi.fn(),
    terminalResize: vi.fn(),
    listSessions: vi.fn(),
    listFolders: vi.fn(),
    updateSession: vi.fn(),
  };
});

const saved: SessionDto = {
  id: 7,
  name: "Servidor",
  host: "saved.example.com",
  port: 22,
  username: "alice",
  auth: { kind: "password", remember: true },
  folder: null,
  passwordSaved: true,
};

function cancelDialog(): void {
  screen.getByRole("dialog").dispatchEvent(new Event("cancel", { bubbles: true, cancelable: true }));
}

describe("SecretDialog", () => {
  afterEach(cleanup);

  it("should show an empty password field without remember when kind is passphrase", () => {
    render(<SecretDialog kind="passphrase" offerRemember onSubmit={vi.fn()} onCancel={vi.fn()} />);

    const input = screen.getByLabelText("Passphrase") as HTMLInputElement;
    expect(input.value).toBe("");
    expect(input.type).toBe("password");
    expect(input.autocomplete).toBe("off");
    expect(screen.queryByLabelText("Recordar")).toBeNull();
  });

  it("should allow paste and prevent copy and cut on the secret field", () => {
    render(<SecretDialog kind="password" offerRemember={false} onSubmit={vi.fn()} onCancel={vi.fn()} />);

    const input = screen.getByLabelText("Contraseña") as HTMLInputElement;
    const paste = new Event("paste", { bubbles: true, cancelable: true });
    input.dispatchEvent(paste);
    fireEvent.change(input, { target: { value: "pegada" } });
    expect(paste.defaultPrevented).toBe(false);
    expect(input.value).toBe("pegada");
    for (const type of ["copy", "cut"]) {
      const event = new Event(type, { bubbles: true, cancelable: true });
      input.dispatchEvent(event);
      expect(event.defaultPrevented).toBe(true);
    }
  });

  it("should clear the field after submit", () => {
    const onSubmit = vi.fn();
    render(<SecretDialog kind="password" offerRemember={false} onSubmit={onSubmit} onCancel={vi.fn()} />);

    const input = screen.getByLabelText("Contraseña") as HTMLInputElement;
    fireEvent.change(input, { target: { value: "s3cret" } });
    fireEvent.click(screen.getByRole("button", { name: "Conectar" }));
    expect(onSubmit).toHaveBeenCalledWith("s3cret", false);
    expect(input.value).toBe("");
  });

  it("should call onCancel when Escape cancels the dialog", () => {
    const onCancel = vi.fn();
    render(<SecretDialog kind="password" offerRemember={false} onSubmit={vi.fn()} onCancel={onCancel} />);

    cancelDialog();
    expect(onCancel).toHaveBeenCalledTimes(1);
  });
});

describe("HostKeyDialog", () => {
  afterEach(cleanup);

  it("should offer Accept for a new fingerprint", () => {
    const onAccept = vi.fn();
    render(
      <HostKeyDialog
        host="example.com"
        fingerprint="SHA256:abc"
        changed={false}
        onAccept={onAccept}
        onCancel={vi.fn()}
      />,
    );

    expect(screen.getByText(/example.com/)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Aceptar" }));
    expect(onAccept).toHaveBeenCalledTimes(1);
  });

  it("should hide Accept and offer Close for a changed fingerprint", () => {
    render(
      <HostKeyDialog
        host="example.com"
        fingerprint="SHA256:bad"
        changed
        onAccept={vi.fn()}
        onCancel={vi.fn()}
      />,
    );

    expect(screen.getByText("La huella del host cambió")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Aceptar" })).toBeNull();
    expect(screen.getByRole("button", { name: "Cerrar" })).toBeTruthy();
  });

  it("should call onCancel when Escape cancels a new fingerprint", () => {
    const onCancel = vi.fn();
    render(
      <HostKeyDialog
        host="example.com"
        fingerprint="SHA256:abc"
        changed={false}
        onAccept={vi.fn()}
        onCancel={onCancel}
      />,
    );

    cancelDialog();
    expect(onCancel).toHaveBeenCalledTimes(1);
  });
});

describe("connection prompts", () => {
  beforeEach(() => {
    vi.mocked(connect).mockReset();
    vi.mocked(disconnect).mockReset();
    vi.mocked(closeTab).mockReset();
    vi.mocked(updateSession).mockReset();
    vi.mocked(listSessions).mockResolvedValue([]);
    vi.mocked(listFolders).mockResolvedValue([]);
    vi.mocked(updateSession).mockResolvedValue(undefined);
    vi.mocked(disconnect).mockResolvedValue(undefined);
    vi.mocked(closeTab).mockResolvedValue(undefined);
  });

  afterEach(cleanup);

  it("should ask again with remember and update the saved password when auth is rejected", async () => {
    const error: CommandError = { kind: "auth_rejected", message: "Credenciales rechazadas" };
    vi.mocked(connect)
      .mockRejectedValueOnce(error)
      .mockResolvedValueOnce({ kind: "connected", connectionId: 31, channelId: 41 });
    vi.mocked(listSessions).mockResolvedValue([saved]);

    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "Conectar Servidor" }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByLabelText("Recordar")).toBeTruthy();

    fireEvent.change(within(dialog).getByLabelText("Contraseña"), { target: { value: "nueva" } });
    fireEvent.click(within(dialog).getByLabelText("Recordar"));
    fireEvent.click(within(dialog).getByRole("button", { name: "Conectar" }));

    await waitFor(() => expect(updateSession).toHaveBeenCalledTimes(1));
    const call = vi.mocked(updateSession).mock.calls[0];
    if (call === undefined) {
      throw new Error("updateSession call missing");
    }
    expect(call[0]).toBe(7);
    expect(call[1]).toMatchObject({ name: "Servidor", host: "saved.example.com", username: "alice" });
    expect(call[2]).toEqual({ kind: "set", value: "nueva" });
    expect(vi.mocked(connect).mock.calls[1]?.[1].password).toBe("nueva");
  });

  it("should leave the tab in error with Retry when the secret dialog is cancelled", async () => {
    vi.mocked(connect).mockResolvedValueOnce({ kind: "needs_passphrase" });

    render(<App />);
    fireEvent.change(screen.getByLabelText("Host"), { target: { value: "example.com" } });
    fireEvent.change(screen.getByLabelText("Puerto"), { target: { value: "22" } });
    fireEvent.change(screen.getByLabelText("Usuario"), { target: { value: "alice" } });
    fireEvent.click(screen.getByRole("button", { name: "Conectar" }));

    const dialog = await screen.findByRole("dialog");
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancelar" }));
    expect(await screen.findByText("Autenticación cancelada")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Reintentar" })).toBeTruthy();
  });
});
