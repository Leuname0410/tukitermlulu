import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "../App";
import type { ConnectArgs, FolderDto, SessionDto } from "../lib/api";
import {
  closeTab,
  connect,
  createSession,
  deleteFolder,
  deleteSession,
  disconnect,
  duplicateSession,
  listFolders,
  listSessions,
  saveFolder,
  updateSession,
} from "../lib/api";
import { SessionForm } from "./SessionForm";
import { buildTree, filterTree, sessionTarget } from "./sessionTree";

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
    createSession: vi.fn(),
    updateSession: vi.fn(),
    duplicateSession: vi.fn(),
    deleteSession: vi.fn(),
    saveFolder: vi.fn(),
    deleteFolder: vi.fn(),
  };
});

vi.mock("../terminal/TerminalView", () => ({
  TerminalView: (props: { channelId: number }) => <div data-testid={`terminal-${props.channelId}`} />,
}));

function makeSession(overrides: Partial<SessionDto> = {}): SessionDto {
  return {
    id: 7,
    name: "Servidor",
    host: "servidor.example",
    port: 22,
    username: "alice",
    auth: { kind: "password", remember: false },
    folder: null,
    passwordSaved: false,
    ...overrides,
  };
}

function makeFolder(overrides: Partial<FolderDto> = {}): FolderDto {
  return { id: 1, name: "Proyectos", parent: null, ...overrides };
}

function first<T>(values: readonly T[]): T {
  const value = values[0];
  if (value === undefined) {
    throw new Error("expected a non-empty list");
  }
  return value;
}

function connectCall(index = 0): [number, ConnectArgs] {
  const call = vi.mocked(connect).mock.calls[index];
  if (!call) {
    throw new Error(`connect call ${index} missing`);
  }
  return [call[0], call[1]];
}

function fillForm(form: HTMLElement, overrides: Partial<{ name: string; host: string; port: string; username: string }> = {}): void {
  const field = (label: string) => within(form).getByLabelText(label);
  fireEvent.change(field("Nombre"), { target: { value: overrides.name ?? "Nueva" } });
  fireEvent.change(field("Host"), { target: { value: overrides.host ?? "example.com" } });
  fireEvent.change(field("Puerto"), { target: { value: overrides.port ?? "22" } });
  fireEvent.change(field("Usuario"), { target: { value: overrides.username ?? "alice" } });
}

async function renderApp(sessions: readonly SessionDto[] = [], folders: readonly FolderDto[] = []): Promise<void> {
  vi.mocked(listSessions).mockResolvedValue([...sessions]);
  vi.mocked(listFolders).mockResolvedValue([...folders]);
  render(<App />);
  await screen.findByRole("tree", { name: "Sesiones" });
}

async function openCreateForm(): Promise<HTMLElement> {
  fireEvent.click(screen.getByRole("button", { name: "Nueva sesión" }));
  return screen.findByRole("form", { name: "Nueva sesión" });
}

async function openEditForm(name = "Servidor"): Promise<HTMLElement> {
  fireEvent.click(await screen.findByRole("button", { name: `Editar sesión ${name}` }));
  return screen.findByRole("form", { name: "Editar sesión" });
}

const originalConfirm = window.confirm;

describe("sessionTree", () => {
  beforeEach(() => {
    vi.mocked(connect).mockReset();
    vi.mocked(connect).mockResolvedValue({ kind: "connected", connectionId: 30, channelId: 40 });
    vi.mocked(disconnect).mockResolvedValue(undefined);
    vi.mocked(closeTab).mockResolvedValue(undefined);
    vi.mocked(createSession).mockResolvedValue(makeSession());
    vi.mocked(updateSession).mockResolvedValue(undefined);
    vi.mocked(duplicateSession).mockResolvedValue(makeSession());
    vi.mocked(deleteSession).mockResolvedValue(undefined);
    vi.mocked(saveFolder).mockResolvedValue(makeFolder());
    vi.mocked(deleteFolder).mockResolvedValue(undefined);
    window.confirm = vi.fn(() => true);
  });

  afterEach(() => {
    cleanup();
    window.confirm = originalConfirm;
  });

  it("should build a nested alphabetical tree and send orphans to the root when the parent is missing", () => {
    const folders = [
      makeFolder({ id: 2, name: "Servidores", parent: 1 }),
      makeFolder({ id: 1, name: "Proyectos" }),
      makeFolder({ id: 3, name: "Zeta", parent: 999 }),
    ];
    const sessions = [
      makeSession({ id: 3, name: "BBB", folder: 2 }),
      makeSession({ id: 1, name: "AAA", folder: 2 }),
      makeSession({ id: 2, name: "Root", folder: 999 }),
      makeSession({ id: 4, name: "Beta", folder: null }),
    ];

    const tree = buildTree(folders, sessions);

    expect(tree.folder).toBeNull();
    expect(tree.folders.map((node) => node.folder?.name)).toEqual(["Proyectos", "Zeta"]);
    const proyectos = first(tree.folders);
    expect(first(proyectos.folders).sessions.map((item) => item.name)).toEqual(["AAA", "BBB"]);
    expect(tree.sessions.map((item) => item.name)).toEqual(["Beta", "Root"]);
  });

  it("should filter by host and username ignoring case and accents while keeping ancestor folders", () => {
    const folders = [
      makeFolder({ id: 1, name: "Proyectos" }),
      makeFolder({ id: 2, name: "Servidores", parent: 1 }),
    ];
    const matched = makeSession({ id: 10, name: "Café", host: "cafe.example", username: "Álvaro", folder: 2 });
    const tree = buildTree(folders, [matched, makeSession({ id: 11, name: "Otro", folder: 1 })]);

    expect(first(first(first(filterTree(tree, "CAFÉ").folders).folders).sessions).id).toBe(10);
    expect(first(first(filterTree(tree, "ALVARO").folders).folders).sessions).toEqual([matched]);
    expect(first(first(filterTree(tree, "cafe.example").folders).folders).sessions).toEqual([matched]);
    expect(filterTree(tree, "   ")).toBe(tree);
  });

  it("should include the sessionId in the connect target", () => {
    const session = makeSession({ id: 42 });
    expect(sessionTarget(session)).toMatchObject({ sessionId: 42, host: session.host, port: 22 });
  });
});

describe("SessionForm", () => {
  afterEach(() => {
    cleanup();
  });

  it("should disable Save when the port is 0 or the host is empty", () => {
    render(<SessionForm folders={[]} onSubmit={() => Promise.resolve()} onCancel={() => undefined} />);
    const form = screen.getByRole("form", { name: "Nueva sesión" });
    const save = within(form).getByRole("button", { name: "Guardar" });
    fillForm(form);
    fireEvent.change(within(form).getByLabelText("Puerto"), { target: { value: "0" } });
    expect(save.hasAttribute("disabled")).toBe(true);
    fireEvent.change(within(form).getByLabelText("Puerto"), { target: { value: "22" } });
    fireEvent.change(within(form).getByLabelText("Host"), { target: { value: "  " } });
    expect(save.hasAttribute("disabled")).toBe(true);
  });

  it("should not render a Passphrase field when the method is key", () => {
    render(
      <SessionForm
        initial={makeSession({ auth: { kind: "key", path: "/tmp/id", hasPassphrase: true } })}
        folders={[]}
        onSubmit={() => Promise.resolve()}
        onCancel={() => undefined}
      />,
    );
    expect(screen.queryByLabelText("Passphrase")).toBeNull();
    expect(screen.getByLabelText("La llave tiene passphrase")).toBeTruthy();
  });

  it("should clear the password from local state after submitting", async () => {
    const onSubmit = vi.fn(() => Promise.resolve());
    render(<SessionForm folders={[]} onSubmit={onSubmit} onCancel={() => undefined} />);
    const form = screen.getByRole("form", { name: "Nueva sesión" });
    fillForm(form);
    const password = within(form).getByLabelText("Contraseña") as HTMLInputElement;
    fireEvent.change(password, { target: { value: "s3cret" } });
    fireEvent.click(within(form).getByRole("button", { name: "Guardar" }));
    await waitFor(() => expect(password.value).toBe(""));
    expect(onSubmit).toHaveBeenCalledWith(expect.objectContaining({ name: "Nueva" }), { kind: "set", value: "s3cret" });
  });
});

describe("App session management", () => {
  beforeEach(() => {
    vi.mocked(listSessions).mockClear();
    vi.mocked(listFolders).mockClear();
    vi.mocked(connect).mockReset();
    vi.mocked(connect).mockResolvedValue({ kind: "connected", connectionId: 30, channelId: 40 });
    vi.mocked(disconnect).mockResolvedValue(undefined);
    vi.mocked(closeTab).mockResolvedValue(undefined);
    vi.mocked(createSession).mockResolvedValue(makeSession());
    vi.mocked(updateSession).mockResolvedValue(undefined);
    vi.mocked(duplicateSession).mockResolvedValue(makeSession());
    vi.mocked(deleteSession).mockResolvedValue(undefined);
    vi.mocked(saveFolder).mockResolvedValue(makeFolder());
    vi.mocked(deleteFolder).mockResolvedValue(undefined);
    window.confirm = vi.fn(() => true);
  });

  afterEach(() => {
    cleanup();
    window.confirm = originalConfirm;
  });

  it("should create with password when remember is checked and without password otherwise", async () => {
    await renderApp();
    const form = await openCreateForm();
    fillForm(form, { name: "Prod", host: "prod.example" });
    fireEvent.click(within(form).getByLabelText("Recordar contraseña"));
    fireEvent.change(within(form).getByLabelText("Contraseña"), { target: { value: "s3cret" } });
    fireEvent.click(within(form).getByRole("button", { name: "Guardar" }));
    await waitFor(() => expect(createSession).toHaveBeenCalledTimes(1));
    const remembered = vi.mocked(createSession).mock.calls[0];
    expect(remembered?.[0]).toMatchObject({ name: "Prod", auth: { kind: "password", remember: true } });
    expect(remembered?.[1]).toBe("s3cret");

    cleanup();
    vi.mocked(createSession).mockClear();
    await renderApp();
    const second = await openCreateForm();
    fillForm(second);
    fireEvent.click(within(second).getByRole("button", { name: "Guardar" }));
    await waitFor(() => expect(createSession).toHaveBeenCalledTimes(1));
    expect(vi.mocked(createSession).mock.calls[0]).toHaveLength(1);
  });

  it("should send keep, set and forget password kinds when editing", async () => {
    await renderApp([makeSession({ passwordSaved: true })]);
    let form = await openEditForm();
    expect((within(form).getByLabelText("Contraseña") as HTMLInputElement).value).toBe("");
    expect(within(form).getByText("Contraseña guardada")).toBeTruthy();
    fireEvent.click(within(form).getByRole("button", { name: "Guardar" }));
    await waitFor(() => expect(updateSession).toHaveBeenCalledWith(7, expect.objectContaining({}), { kind: "keep" }));

    cleanup();
    vi.mocked(updateSession).mockClear();
    await renderApp([makeSession()]);
    form = await openEditForm();
    fireEvent.change(within(form).getByLabelText("Contraseña"), { target: { value: "nueva" } });
    fireEvent.click(within(form).getByRole("button", { name: "Guardar" }));
    await waitFor(() => expect(updateSession).toHaveBeenCalledWith(7, expect.objectContaining({}), { kind: "set", value: "nueva" }));

    cleanup();
    vi.mocked(updateSession).mockClear();
    await renderApp([makeSession({ passwordSaved: true })]);
    form = await openEditForm();
    fireEvent.click(within(form).getByRole("button", { name: "Olvidar contraseña guardada" }));
    fireEvent.click(within(form).getByRole("button", { name: "Guardar" }));
    await waitFor(() => expect(updateSession).toHaveBeenCalledWith(7, expect.objectContaining({}), { kind: "forget" }));
  });

  it("should show an invalid_input error near the form when the backend rejects it", async () => {
    vi.mocked(createSession).mockRejectedValueOnce({ kind: "invalid_input", message: "El host no es válido" });
    await renderApp();
    const form = await openCreateForm();
    fillForm(form);
    fireEvent.click(within(form).getByRole("button", { name: "Guardar" }));
    expect(await screen.findByText("El host no es válido")).toBeTruthy();
  });

  it("should reload both lists after duplicating or deleting a session", async () => {
    await renderApp([makeSession()]);
    fireEvent.click(await screen.findByRole("button", { name: "Duplicar sesión Servidor" }));
    await waitFor(() => {
      expect(duplicateSession).toHaveBeenCalledWith(7);
      expect(listSessions).toHaveBeenCalledTimes(2);
      expect(listFolders).toHaveBeenCalledTimes(2);
    });

    cleanup();
    vi.mocked(listSessions).mockClear();
    vi.mocked(listFolders).mockClear();
    await renderApp([makeSession()]);
    fireEvent.click(await screen.findByRole("button", { name: "Borrar sesión Servidor" }));
    await waitFor(() => {
      expect(deleteSession).toHaveBeenCalledWith(7);
      expect(listSessions).toHaveBeenCalledTimes(2);
      expect(listFolders).toHaveBeenCalledTimes(2);
    });
  });

  it("should filter the tree by search and toggle folder expansion", async () => {
    await renderApp(
      [makeSession({ id: 10, name: "Café" }), makeSession({ id: 11, name: "Otro" })],
      [makeFolder()],
    );
    const folderItem = await screen.findByRole("treeitem", { name: "Carpeta Proyectos" });
    expect(folderItem.getAttribute("aria-expanded")).toBe("true");
    fireEvent.click(within(folderItem).getByRole("button", { name: "Plegar Proyectos" }));
    expect(screen.getByRole("treeitem", { name: "Carpeta Proyectos" }).getAttribute("aria-expanded")).toBe("false");

    fireEvent.change(screen.getByRole("searchbox", { name: "Buscar sesiones" }), {
      target: { value: "CAFÉ" },
    });
    expect(screen.getByRole("treeitem", { name: "Sesión Café" })).toBeTruthy();
    expect(screen.queryByRole("treeitem", { name: "Sesión Otro" })).toBeNull();
  });

  it("should open a tab with sessionId when double-clicking or pressing Enter on a session", async () => {
    await renderApp([makeSession()]);
    const item = await screen.findByRole("treeitem", { name: "Sesión Servidor" });
    fireEvent.doubleClick(item);
    await screen.findByRole("tab", { name: /alice@servidor.example/i });
    expect(connectCall(0)[1]).toMatchObject({ sessionId: 7, host: "servidor.example", port: 22 });

    cleanup();
    await renderApp([makeSession()]);
    const secondItem = await screen.findByRole("treeitem", { name: "Sesión Servidor" });
    fireEvent.keyDown(secondItem, { key: "Enter" });
    await waitFor(() => expect(connect).toHaveBeenCalledTimes(2));
    expect(connectCall(1)[1]).toMatchObject({ sessionId: 7 });
  });
});
