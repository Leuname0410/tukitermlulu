import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { EntryDto, TransferDto } from "../lib/api";
import { SftpSidebar } from "./SftpSidebar";
import { TransferList } from "./TransferList";

const mocks = vi.hoisted(() => ({
  cancelTransfer: vi.fn(),
  clearTransfers: vi.fn(),
  downloadsDir: vi.fn(),
  enqueueDownload: vi.fn(),
  enqueueUpload: vi.fn(),
  onFileDrop: vi.fn(),
  onTransfers: vi.fn(),
  sftpChmod: vi.fn(),
  sftpHome: vi.fn(),
  sftpList: vi.fn(),
  sftpMkdir: vi.fn(),
  sftpRemove: vi.fn(),
  sftpRename: vi.fn(),
}));

vi.mock("../lib/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../lib/api")>();
  return { ...actual, ...mocks };
});

function file(name = "notes.txt", permissions?: number): EntryDto {
  return { name, path: `/home/alice/${name}`, isDir: false, size: 12, permissions };
}

function rowNamed(name: string): HTMLElement {
  const row = within(screen.getByRole("grid"))
    .getAllByRole("row")
    .find((candidate) => candidate.textContent?.includes(name));
  if (!row) throw new Error(`Falta la fila de ${name}`);
  return row;
}

async function showSidebar(entries: readonly EntryDto[] = [file()]): Promise<void> {
  mocks.sftpList.mockResolvedValue([...entries]);
  render(<SftpSidebar connectionId={1} />);
  await screen.findByRole("grid");
}

function openMenu(name: string): void {
  fireEvent.contextMenu(rowNamed(name), { clientX: 12, clientY: 34 });
}

function menu(): HTMLElement {
  return screen.getByRole("menu");
}

function nameInput(): HTMLInputElement {
  return screen.getByLabelText("Nuevo nombre") as HTMLInputElement;
}

function running(): TransferDto {
  return {
    id: 7,
    connectionId: 1,
    direction: "download",
    remote: "/home/alice/big.bin",
    local: "C:/big.bin",
    state: "running",
    transferred: 5,
    total: 10,
    error: null,
  };
}

function transferHandler(): (jobs: TransferDto[]) => void {
  const handler = mocks.onTransfers.mock.calls[0]?.[0];
  if (typeof handler !== "function") throw new Error("onTransfers no fue invocado");
  return handler as (jobs: TransferDto[]) => void;
}

describe("menú contextual SFTP", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.sftpHome.mockResolvedValue("/home/alice");
    mocks.sftpList.mockResolvedValue([file()]);
    mocks.downloadsDir.mockResolvedValue("C:/Descargas");
    mocks.enqueueDownload.mockResolvedValue(1);
    mocks.enqueueUpload.mockResolvedValue(1);
    mocks.onFileDrop.mockResolvedValue(() => {});
    mocks.onTransfers.mockResolvedValue(() => {});
    mocks.sftpRename.mockResolvedValue(undefined);
    mocks.sftpRemove.mockResolvedValue(undefined);
    mocks.sftpMkdir.mockResolvedValue(undefined);
    mocks.sftpChmod.mockResolvedValue(undefined);
    mocks.cancelTransfer.mockResolvedValue(undefined);
    mocks.clearTransfers.mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText: vi.fn().mockResolvedValue(undefined) },
    });
    window.confirm = vi.fn(() => true);
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it("should enqueue a download in the downloads dir and refresh when Descargar is chosen", async () => {
    await showSidebar();
    mocks.sftpList.mockClear();
    openMenu("notes.txt");

    fireEvent.click(within(menu()).getByRole("menuitem", { name: "Descargar" }));

    await waitFor(() =>
      expect(mocks.enqueueDownload).toHaveBeenCalledWith(
        1,
        "/home/alice/notes.txt",
        "C:/Descargas",
      ),
    );
    expect(mocks.sftpList).toHaveBeenCalledWith(1, "/home/alice");
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("should rename inside the same folder and refresh with a valid name", async () => {
    await showSidebar();
    mocks.sftpList.mockClear();
    openMenu("notes.txt");
    fireEvent.click(within(menu()).getByRole("menuitem", { name: "Renombrar" }));

    fireEvent.change(nameInput(), { target: { value: "nuevo.txt" } });
    fireEvent.keyDown(nameInput(), { key: "Enter" });

    await waitFor(() =>
      expect(mocks.sftpRename).toHaveBeenCalledWith(
        1,
        "/home/alice/notes.txt",
        "/home/alice/nuevo.txt",
      ),
    );
    expect(mocks.sftpList).toHaveBeenCalledWith(1, "/home/alice");
  });

  it.each(["", "a/b", ".", ".."])(
    "should reject the invalid name %s without calling sftpRename",
    async (name) => {
      await showSidebar();
      openMenu("notes.txt");
      fireEvent.click(within(menu()).getByRole("menuitem", { name: "Renombrar" }));

      fireEvent.change(nameInput(), { target: { value: name } });
      fireEvent.keyDown(nameInput(), { key: "Enter" });

      expect(mocks.sftpRename).not.toHaveBeenCalled();
      expect(screen.getByRole("alert").textContent).toContain("nombre");
    },
  );

  it("should remove the entry after an accepted confirm and refresh", async () => {
    await showSidebar();
    mocks.sftpList.mockClear();
    openMenu("notes.txt");

    fireEvent.click(within(menu()).getByRole("menuitem", { name: "Borrar" }));

    expect(window.confirm).toHaveBeenCalledWith("¿Borrar notes.txt?");
    await waitFor(() => expect(mocks.sftpRemove).toHaveBeenCalledWith(1, "/home/alice/notes.txt"));
    expect(mocks.sftpList).toHaveBeenCalledWith(1, "/home/alice");
  });

  it("should not remove the entry when confirm is cancelled", async () => {
    await showSidebar();
    window.confirm = vi.fn(() => false);
    openMenu("notes.txt");

    fireEvent.click(within(menu()).getByRole("menuitem", { name: "Borrar" }));

    expect(mocks.sftpRemove).not.toHaveBeenCalled();
  });

  it("should create a folder in the current directory from the background menu", async () => {
    await showSidebar();
    mocks.sftpList.mockClear();
    fireEvent.contextMenu(screen.getByLabelText("Explorador SFTP"), { clientX: 5, clientY: 6 });
    fireEvent.click(within(menu()).getByRole("menuitem", { name: "Nueva carpeta" }));

    fireEvent.change(nameInput(), { target: { value: "docs" } });
    fireEvent.keyDown(nameInput(), { key: "Enter" });

    await waitFor(() => expect(mocks.sftpMkdir).toHaveBeenCalledWith(1, "/home/alice/docs"));
    expect(mocks.sftpList).toHaveBeenCalledWith(1, "/home/alice");
  });

  it("should copy the entry path to the clipboard", async () => {
    await showSidebar();
    openMenu("notes.txt");

    fireEvent.click(within(menu()).getByRole("menuitem", { name: "Copiar ruta" }));

    expect(navigator.clipboard.writeText).toHaveBeenCalledWith("/home/alice/notes.txt");
  });

  it("should hide Permisos when the server did not report permissions", async () => {
    await showSidebar();
    openMenu("notes.txt");
    expect(within(menu()).queryByRole("menuitem", { name: /Permisos/ })).toBeNull();
  });

  it("should sync rwx checkboxes with the octal mode and chmod on apply", async () => {
    await showSidebar([file("notes.txt", 0o754)]);
    mocks.sftpList.mockClear();
    openMenu("notes.txt");
    fireEvent.click(within(menu()).getByRole("menuitem", { name: /Permisos/ }));

    const dialog = await screen.findByRole("dialog", { name: "Permisos" });
    const octal = within(dialog).getByLabelText("Octal") as HTMLInputElement;
    expect(octal.value).toBe("754");
    expect((within(dialog).getByLabelText("Propietario lectura") as HTMLInputElement).checked).toBe(
      true,
    );
    expect((within(dialog).getByLabelText("Grupo lectura") as HTMLInputElement).checked).toBe(true);
    expect((within(dialog).getByLabelText("Grupo escritura") as HTMLInputElement).checked).toBe(
      false,
    );
    expect((within(dialog).getByLabelText("Otros lectura") as HTMLInputElement).checked).toBe(true);

    fireEvent.click(within(dialog).getByLabelText("Grupo escritura"));
    expect(octal.value).toBe("774");

    fireEvent.change(octal, { target: { value: "0o640" } });
    expect((within(dialog).getByLabelText("Otros lectura") as HTMLInputElement).checked).toBe(false);
    expect((within(dialog).getByLabelText("Grupo lectura") as HTMLInputElement).checked).toBe(true);

    fireEvent.click(within(dialog).getByRole("button", { name: "Aplicar" }));
    await waitFor(() =>
      expect(mocks.sftpChmod).toHaveBeenCalledWith(1, "/home/alice/notes.txt", 0o640),
    );
    expect(mocks.sftpList).toHaveBeenCalledWith(1, "/home/alice");
  });

  it("should not chmod when the octal mode is outside 0-7777", async () => {
    await showSidebar([file("notes.txt", 0o754)]);
    openMenu("notes.txt");
    fireEvent.click(within(menu()).getByRole("menuitem", { name: /Permisos/ }));

    const dialog = await screen.findByRole("dialog", { name: "Permisos" });
    fireEvent.change(within(dialog).getByLabelText("Octal"), { target: { value: "8888" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Aplicar" }));

    expect(mocks.sftpChmod).not.toHaveBeenCalled();
    expect(within(dialog).getByRole("alert").textContent).toContain("7777");
  });
});

describe("TransferList", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.cancelTransfer.mockResolvedValue(undefined);
    mocks.clearTransfers.mockResolvedValue(undefined);
    mocks.onTransfers.mockResolvedValue(() => {});
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it("should show progress for a running event, cancel it and clear finished transfers", async () => {
    render(<TransferList />);
    act(() => transferHandler()([running()]));

    const progress = await screen.findByRole("progressbar");
    expect(progress.getAttribute("value")).toBe("5");
    expect(progress.getAttribute("max")).toBe("10");
    expect(screen.getByText("/home/alice/big.bin")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "Cancelar" }));
    await waitFor(() => expect(mocks.cancelTransfer).toHaveBeenCalledWith(7));

    fireEvent.click(screen.getByRole("button", { name: "Limpiar terminadas" }));
    await waitFor(() => expect(mocks.clearTransfers).toHaveBeenCalledOnce());
  });

  it("should mount the transfer list under the sidebar", async () => {
    mocks.sftpHome.mockResolvedValue("/home/alice");
    mocks.sftpList.mockResolvedValue([file()]);
    mocks.onFileDrop.mockResolvedValue(() => {});
    render(<SftpSidebar connectionId={1} />);
    expect(await screen.findByRole("region", { name: "Transferencias" })).toBeTruthy();
  });
});
