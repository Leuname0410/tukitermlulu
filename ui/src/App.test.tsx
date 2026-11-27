import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";

const { exportDiagnostics } = vi.hoisted(() => ({
  exportDiagnostics: vi.fn<() => Promise<string>>(),
}));

vi.mock("./lib/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./lib/api")>();
  return {
    ...actual,
    listSessions: vi.fn(async () => []),
    listFolders: vi.fn(async () => []),
    exportDiagnostics,
  };
});

describe("App diagnostics", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    cleanup();
  });

  it("should show the exported path when clicking Exportar diagnóstico", async () => {
    const path = "C:/Users/alice/Downloads/tukitermlulu-diagnostico-20260101-010101.txt";
    exportDiagnostics.mockResolvedValueOnce(path);
    render(<App />);

    fireEvent.click(screen.getByRole("button", { name: "Exportar diagnóstico" }));

    expect(exportDiagnostics).toHaveBeenCalledTimes(1);
    const status = await screen.findByRole("status");
    expect(status.textContent).toContain(path);
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("should show an error when exporting diagnostics fails", async () => {
    exportDiagnostics.mockRejectedValueOnce(new Error("No se pudo escribir el diagnóstico"));
    render(<App />);

    fireEvent.click(screen.getByRole("button", { name: "Exportar diagnóstico" }));

    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("No se pudo escribir el diagnóstico");
    expect(screen.queryByRole("status")).toBeNull();
  });
});
