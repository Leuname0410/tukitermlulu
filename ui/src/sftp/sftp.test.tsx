import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { CommandError, EntryDto, FileDropEvent, TransferDto } from "../lib/api";
import { sftpHome, sftpList } from "../lib/api";
import { SftpSidebar } from "./SftpSidebar";
import { formatSize, parentPath, sortEntries } from "./sidebarModel";
import { toCssPoint } from "./useFileDrop";

const { enqueueUpload, onFileDrop, onTransfers } = vi.hoisted(() => ({
  enqueueUpload: vi.fn(),
  onFileDrop: vi.fn(),
  onTransfers: vi.fn(),
}));

vi.mock("../lib/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../lib/api")>();
  return {
    ...actual,
    enqueueUpload,
    onFileDrop,
    onTransfers,
    sftpHome: vi.fn(),
    sftpList: vi.fn(),
  };
});

function entry(name: string, isDir = false, size = 0, path = `/home/alice/${name}`): EntryDto {
  return { name, path, isDir, size };
}

function rowNamed(grid: HTMLElement, name: string): HTMLElement {
  const row = within(grid)
    .getAllByRole("row")
    .find((candidate) => candidate.textContent?.includes(name));
  if (!row) {
    throw new Error(`Falta la fila de ${name}`);
  }
  return row;
}

function rows(grid: HTMLElement): string[] {
  return within(grid)
    .getAllByRole("row")
    .map((row) => row.textContent ?? "");
}

function dropHandler(): (event: FileDropEvent) => void {
  const handler = onFileDrop.mock.calls[0]?.[0];
  if (typeof handler !== "function") throw new Error("onFileDrop no fue invocado");
  return handler as (event: FileDropEvent) => void;
}

function transferHandler(): (jobs: TransferDto[]) => void {
  const handler = onTransfers.mock.calls[0]?.[0];
  if (typeof handler !== "function") throw new Error("onTransfers no fue invocado");
  return handler as (jobs: TransferDto[]) => void;
}

function sidebarRect(width: number, height: number): void {
  screen.getByLabelText("Explorador SFTP").getBoundingClientRect = () =>
    ({
      left: 10,
      top: 10,
      right: 10 + width,
      bottom: 10 + height,
      width,
      height,
      x: 10,
      y: 10,
      toJSON: () => ({}),
    }) as DOMRect;
}

function dpr(value: number): void {
  Object.defineProperty(window, "devicePixelRatio", { configurable: true, value });
}

function done(remote: string): TransferDto {
  return {
    id: 5,
    connectionId: 1,
    direction: "upload",
    remote,
    local: "C:/a.txt",
    state: "done",
    transferred: 4,
    total: 4,
    error: null,
  };
}

describe("sidebarModel", () => {
  it("should sort folders first with numeric name comparison when sorting by name or size", () => {
    const entries = [
      entry("file10", false, 10),
      entry("zeta", true, 0),
      entry("file2", false, 2),
      entry("alpha", true, 0),
    ];

    expect(sortEntries(entries, "name", true).map((item) => item.name)).toEqual([
      "alpha",
      "zeta",
      "file2",
      "file10",
    ]);
    expect(sortEntries(entries, "name", false).map((item) => item.name)).toEqual([
      "zeta",
      "alpha",
      "file10",
      "file2",
    ]);
    expect(sortEntries(entries, "size", true).map((item) => item.name)).toEqual([
      "alpha",
      "zeta",
      "file2",
      "file10",
    ]);
    expect(sortEntries(entries, "size", false).map((item) => item.name)).toEqual([
      "zeta",
      "alpha",
      "file10",
      "file2",
    ]);
  });

  it("should return the parent path for each case", () => {
    expect(parentPath("/")).toBeNull();
    expect(parentPath("/a")).toBe("/");
    expect(parentPath("/a/b")).toBe("/a");
    expect(parentPath("/C:/x")).toBe("/C:/");
  });

  it("should format byte sizes with binary units", () => {
    expect(formatSize(0)).toBe("0 B");
    expect(formatSize(512)).toBe("512 B");
    expect(formatSize(1024)).toBe("1.0 KiB");
    expect(formatSize(1536)).toBe("1.5 KiB");
    expect(formatSize(2097152)).toBe("2.0 MiB");
    expect(formatSize(1073741824)).toBe("1.0 GiB");
  });
});

describe("SftpSidebar", () => {
  beforeEach(() => {
    vi.mocked(sftpHome).mockReset().mockResolvedValue("/home/alice");
    vi.mocked(sftpList).mockReset().mockResolvedValue([entry("file2", false, 2), entry("docs", true)]);
    enqueueUpload.mockReset().mockResolvedValue(1);
    onFileDrop.mockReset().mockResolvedValue(() => {});
    onTransfers.mockReset().mockResolvedValue(() => {});
    dpr(1);
  });

  afterEach(() => {
    cleanup();
    dpr(1);
    vi.restoreAllMocks();
  });

  it("should call sftpHome then sftpList and show folders first when mounted", async () => {
    render(<SftpSidebar connectionId={1} />);

    expect(await screen.findByText("docs")).toBeTruthy();
    expect(sftpHome).toHaveBeenCalledWith(1);
    expect(sftpList).toHaveBeenCalledWith(1, "/home/alice");
    const grid = screen.getByRole("grid");
    expect(rows(grid)[1]).toContain("docs");
    expect(rows(grid)[2]).toContain("file2");
  });

  it("should list the external path without calling sftpHome when path is provided", async () => {
    render(<SftpSidebar connectionId={2} path="/srv" />);

    expect(await screen.findByText("docs")).toBeTruthy();
    expect(sftpHome).not.toHaveBeenCalled();
    expect(sftpList).toHaveBeenCalledWith(2, "/srv");
  });

  it("should list the child path when double-clicking a folder", async () => {
    render(<SftpSidebar connectionId={1} />);
    const folder = rowNamed(await screen.findByRole("grid"), "docs");

    fireEvent.doubleClick(folder);

    await waitFor(() => {
      expect(sftpList).toHaveBeenLastCalledWith(1, "/home/alice/docs");
    });
  });

  it("should list the child path when pressing Enter on a folder but not on a file", async () => {
    render(<SftpSidebar connectionId={1} />);
    const grid = await screen.findByRole("grid");

    fireEvent.keyDown(rowNamed(grid, "docs"), { key: "Enter" });
    await waitFor(() => {
      expect(sftpList).toHaveBeenLastCalledWith(1, "/home/alice/docs");
    });

    fireEvent.keyDown(rowNamed(grid, "file2"), { key: "Enter" });
    expect(sftpList).toHaveBeenCalledTimes(2);
  });

  it("should list the parent path when clicking Subir de nivel", async () => {
    render(<SftpSidebar connectionId={1} />);
    await screen.findByText("docs");

    fireEvent.click(screen.getByRole("button", { name: "Subir de nivel" }));

    await waitFor(() => {
      expect(sftpList).toHaveBeenLastCalledWith(1, "/home");
    });
  });

  it("should disable Subir de nivel at the root", async () => {
    vi.mocked(sftpHome).mockResolvedValueOnce("/");
    render(<SftpSidebar connectionId={1} />);
    await screen.findByText("docs");

    expect((screen.getByRole("button", { name: "Subir de nivel" }) as HTMLButtonElement).disabled).toBe(
      true,
    );
  });

  it("should relist the same path when clicking Refrescar", async () => {
    render(<SftpSidebar connectionId={1} />);
    await screen.findByText("docs");

    fireEvent.click(screen.getByRole("button", { name: "Refrescar" }));

    await waitFor(() => {
      expect(sftpList).toHaveBeenCalledTimes(2);
    });
    expect(sftpList).toHaveBeenLastCalledWith(1, "/home/alice");
  });

  it("should list the typed path when pressing Enter in the route field", async () => {
    render(<SftpSidebar connectionId={1} />);
    await screen.findByText("docs");

    fireEvent.change(screen.getByLabelText("Ruta remota"), { target: { value: "/tmp" } });
    fireEvent.keyDown(screen.getByLabelText("Ruta remota"), { key: "Enter" });

    await waitFor(() => {
      expect(sftpList).toHaveBeenLastCalledWith(1, "/tmp");
    });
  });

  it("should show the error and keep the previous listing when a reload fails", async () => {
    const error: CommandError = { kind: "permission_denied", message: "Sin permiso" };
    vi.mocked(sftpList)
      .mockResolvedValueOnce([entry("docs", true), entry("notes.txt", false, 10)])
      .mockRejectedValueOnce(error);
    render(<SftpSidebar connectionId={1} />);
    await screen.findByText("docs");

    fireEvent.click(screen.getByRole("button", { name: "Refrescar" }));

    expect((await screen.findByRole("alert")).textContent).toContain("Sin permiso");
    expect(screen.getByText("docs")).toBeTruthy();
    expect(screen.getByText("notes.txt")).toBeTruthy();
  });

  it("should invert the order when clicking Tamaño twice", async () => {
    vi.mocked(sftpList).mockResolvedValueOnce([
      entry("medium.txt", false, 2048),
      entry("small.txt", false, 1),
      entry("big.bin", false, 1048576),
      entry("docs", true),
    ]);
    render(<SftpSidebar connectionId={1} />);
    const grid = await screen.findByRole("grid");

    fireEvent.click(screen.getByRole("button", { name: "Tamaño" }));
    expect(rows(grid)[1]).toContain("docs");
    expect(rows(grid)[2]).toContain("small.txt");
    expect(rows(grid)[4]).toContain("big.bin");

    fireEvent.click(screen.getByRole("button", { name: "Tamaño" }));
    expect(rows(grid)[1]).toContain("docs");
    expect(rows(grid)[2]).toContain("big.bin");
    expect(rows(grid)[4]).toContain("small.txt");
  });

  it("should mark the explorer as busy until the listing resolves", async () => {
    let resolve!: (value: EntryDto[]) => void;
    vi.mocked(sftpList).mockReturnValueOnce(new Promise<EntryDto[]>((res) => {
      resolve = res;
    }));
    render(<SftpSidebar connectionId={1} />);
    await waitFor(() => {
      expect(sftpList).toHaveBeenCalled();
    });

    expect(screen.getByLabelText("Explorador SFTP").getAttribute("aria-busy")).toBe("true");
    resolve([]);
    await waitFor(() => {
      expect(screen.getByLabelText("Explorador SFTP").getAttribute("aria-busy")).toBe("false");
    });
  });
});

describe("SftpSidebar follow terminal", () => {
  beforeEach(() => {
    vi.mocked(sftpHome).mockReset().mockResolvedValue("/home/alice");
    vi.mocked(sftpList).mockReset().mockResolvedValue([entry("docs", true)]);
    enqueueUpload.mockReset().mockResolvedValue(1);
    onFileDrop.mockReset().mockResolvedValue(() => {});
    onTransfers.mockReset().mockResolvedValue(() => {});
    dpr(1);
  });

  afterEach(() => {
    cleanup();
    dpr(1);
    vi.restoreAllMocks();
  });

  function checkbox(): HTMLInputElement {
    return screen.getByRole("checkbox", { name: "Seguir terminal" }) as HTMLInputElement;
  }

  it("should list the new follow path when follow terminal is checked", async () => {
    const { rerender } = render(<SftpSidebar connectionId={1} followPath="/var/log" />);
    expect(await screen.findByText("docs")).toBeTruthy();
    expect(sftpList).toHaveBeenLastCalledWith(1, "/var/log");
    expect(checkbox().checked).toBe(true);

    rerender(<SftpSidebar connectionId={1} followPath="/tmp" />);

    await waitFor(() => expect(sftpList).toHaveBeenLastCalledWith(1, "/tmp"));
  });

  it("should follow the latest path only after follow terminal is checked again", async () => {
    const { rerender } = render(<SftpSidebar connectionId={1} followPath="/var/log" />);
    expect(await screen.findByText("docs")).toBeTruthy();

    fireEvent.click(checkbox());
    expect(checkbox().checked).toBe(false);
    rerender(<SftpSidebar connectionId={1} followPath="/tmp" />);
    await act(async () => {});
    expect(sftpList).not.toHaveBeenCalledWith(1, "/tmp");

    fireEvent.click(checkbox());

    await waitFor(() => expect(sftpList).toHaveBeenLastCalledWith(1, "/tmp"));
  });

  it("should not relist when the follow path is unchanged", async () => {
    const { rerender } = render(<SftpSidebar connectionId={1} followPath="/var/log" />);
    expect(await screen.findByText("docs")).toBeTruthy();
    const calls = vi.mocked(sftpList).mock.calls.length;

    await act(async () => {
      rerender(<SftpSidebar connectionId={1} followPath="/var/log" />);
    });

    expect(sftpList).toHaveBeenCalledTimes(calls);
  });

  it("should keep manual navigation from disabling follow terminal", async () => {
    render(<SftpSidebar connectionId={1} followPath="/var/log" />);
    expect(await screen.findByText("docs")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "Subir de nivel" }));

    await waitFor(() => expect(sftpList).toHaveBeenLastCalledWith(1, "/var"));
    expect(checkbox().checked).toBe(true);
  });

  it("should show the error and keep the listing when a followed path fails", async () => {
    const error: CommandError = { kind: "permission_denied", message: "Sin permiso" };
    vi.mocked(sftpList).mockImplementation((_id, target) =>
      target === "/denied" ? Promise.reject(error) : Promise.resolve([entry("docs", true)]),
    );
    const { rerender } = render(<SftpSidebar connectionId={1} followPath="/var/log" />);
    expect(await screen.findByText("docs")).toBeTruthy();

    rerender(<SftpSidebar connectionId={1} followPath="/denied" />);

    expect((await screen.findByRole("alert")).textContent).toContain("Sin permiso");
    expect(screen.getByText("docs")).toBeTruthy();
  });
});

it("should convert physical pixels to CSS pixels with devicePixelRatio 1 and 2", () => {
  expect(toCssPoint(120, 80, 1)).toEqual({ x: 120, y: 80 });
  expect(toCssPoint(120, 80, 2)).toEqual({ x: 60, y: 40 });
});

describe("SftpSidebar file drops", () => {
  beforeEach(() => {
    vi.mocked(sftpHome).mockReset().mockResolvedValue("/home/alice");
    vi.mocked(sftpList).mockReset().mockResolvedValue([entry("file2", false, 2), entry("docs", true)]);
    enqueueUpload.mockReset().mockResolvedValue(1);
    onFileDrop.mockReset().mockResolvedValue(() => {});
    onTransfers.mockReset().mockResolvedValue(() => {});
    dpr(1);
  });

  afterEach(() => {
    cleanup();
    dpr(1);
    vi.restoreAllMocks();
  });

  async function subscribed(): Promise<void> {
    render(<SftpSidebar connectionId={1} />);
    await screen.findByText("docs");
    await waitFor(() => {
      expect(onFileDrop).toHaveBeenCalled();
      expect(onTransfers).toHaveBeenCalled();
    });
  }

  it("should enqueue each dropped file when the drop lands inside the visible sidebar", async () => {
    dpr(2);
    await subscribed();
    sidebarRect(200, 100);
    act(() => dropHandler()({ kind: "drop", paths: ["C:/a.txt", "C:/b.txt"], x: 100, y: 100 }));
    await waitFor(() => expect(enqueueUpload).toHaveBeenCalledTimes(2));
    expect(enqueueUpload).toHaveBeenNthCalledWith(1, 1, "C:/a.txt", "/home/alice");
    expect(enqueueUpload).toHaveBeenNthCalledWith(2, 1, "C:/b.txt", "/home/alice");
  });

  it("should ignore drops outside the sidebar or on a hidden tab", async () => {
    await subscribed();
    sidebarRect(200, 100);
    act(() => dropHandler()({ kind: "drop", paths: ["C:/a.txt"], x: 300, y: 300 }));
    sidebarRect(0, 0);
    act(() => dropHandler()({ kind: "drop", paths: ["C:/b.txt"], x: 50, y: 50 }));
    expect(enqueueUpload).not.toHaveBeenCalled();
  });

  it("should highlight the sidebar on over and clear the highlight on leave", async () => {
    await subscribed();
    sidebarRect(200, 100);
    const sidebar = screen.getByLabelText("Explorador SFTP");
    act(() => dropHandler()({ kind: "over", x: 50, y: 50 }));
    expect(sidebar.getAttribute("data-drop-active")).toBe("true");
    expect(screen.getByText("Soltar para subir")).toBeTruthy();
    act(() => dropHandler()({ kind: "leave" }));
    expect(sidebar.getAttribute("data-drop-active")).toBe("false");
    expect(screen.queryByText("Soltar para subir")).toBeNull();
  });

  it("should show the error and keep enqueueing the other files when one upload fails", async () => {
    enqueueUpload.mockRejectedValueOnce(new Error("No se pudo subir a.txt")).mockResolvedValue(2);
    await subscribed();
    sidebarRect(200, 100);
    act(() => dropHandler()({ kind: "drop", paths: ["a.txt", "b.txt"], x: 50, y: 50 }));
    await waitFor(() => expect(enqueueUpload).toHaveBeenCalledTimes(2));
    expect((await screen.findByRole("alert")).textContent).toContain("No se pudo subir a.txt");
  });

  it("should warn when the backend rejects a dropped folder", async () => {
    enqueueUpload.mockRejectedValueOnce({ kind: "invalid_input", message: "local path must be a file" });
    await subscribed();
    sidebarRect(200, 100);
    act(() => dropHandler()({ kind: "drop", paths: ["C:/docs"], x: 50, y: 50 }));
    expect((await screen.findByRole("alert")).textContent).toContain("Las carpetas aún no se pueden subir");
  });

  it("should relist the current folder at most once per second when an upload finishes there", async () => {
    const clock = vi.spyOn(Date, "now").mockReturnValue(1_000);
    await subscribed();
    act(() => transferHandler()([done("/home/alice/a.txt")]));
    await waitFor(() => expect(sftpList).toHaveBeenCalledTimes(2));
    expect(sftpList).toHaveBeenLastCalledWith(1, "/home/alice");
    act(() => transferHandler()([done("/srv/a.txt"), done("/home/alice/b.txt")]));
    expect(sftpList).toHaveBeenCalledTimes(2);
    clock.mockReturnValue(3_000);
    act(() => transferHandler()([done("/home/alice/c.txt")]));
    await waitFor(() => expect(sftpList).toHaveBeenCalledTimes(3));
  });
});
