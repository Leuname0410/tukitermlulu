import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { CommandError, EntryDto } from "../lib/api";
import { sftpHome, sftpList } from "../lib/api";
import { SftpSidebar } from "./SftpSidebar";
import { formatSize, parentPath, sortEntries } from "./sidebarModel";

vi.mock("../lib/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../lib/api")>();
  return {
    ...actual,
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
  });

  afterEach(() => {
    cleanup();
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
