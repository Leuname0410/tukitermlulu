import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { SplitPane } from "./SplitPane";
import {
  DEFAULT_SIDEBAR_WIDTH,
  clampWidth,
  loadWidth,
  saveWidth,
} from "./splitModel";

function mockStorage(throwOnSet = false, throwOnGet = false): Storage {
  const data = new Map<string, string>();
  return {
    get length() {
      return data.size;
    },
    clear() {
      data.clear();
    },
    getItem(key: string) {
      if (throwOnGet) {
        throw new Error("storage unavailable");
      }
      return data.get(key) ?? null;
    },
    key(index: number) {
      return Array.from(data.keys())[index] ?? null;
    },
    removeItem(key: string) {
      data.delete(key);
    },
    setItem(key: string, value: string) {
      if (throwOnSet) {
        throw new Error("storage unavailable");
      }
      data.set(key, value);
    },
  };
}

describe("splitModel", () => {
  beforeEach(() => {
    vi.stubGlobal("localStorage", mockStorage());
  });

  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
  });

  it("should clamp width below minimum, above 60%, and within range when clamping", () => {
    expect(clampWidth(100, 1000)).toBe(180);
    expect(clampWidth(700, 1000)).toBe(600);
    expect(clampWidth(320, 1000)).toBe(320);
  });

  it("should use saved width for same session and own or default for others when loading", () => {
    saveWidth(1, 340);
    saveWidth(2, 400);

    expect(loadWidth(1)).toBe(340);
    expect(loadWidth(2)).toBe(400);
    expect(loadWidth(3)).toBe(DEFAULT_SIDEBAR_WIDTH);
    expect(loadWidth(undefined)).toBe(DEFAULT_SIDEBAR_WIDTH);
  });

  it("should return default width without breaking when localStorage throws", () => {
    vi.stubGlobal("localStorage", mockStorage(true, true));

    expect(loadWidth(1)).toBe(DEFAULT_SIDEBAR_WIDTH);
    expect(() => saveWidth(1, 320)).not.toThrow();
  });
});

describe("SplitPane", () => {
  beforeEach(() => {
    vi.stubGlobal("localStorage", mockStorage());
  });

  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
  });

  function renderSplit(sessionId?: number): HTMLElement {
    render(
      <div style={{ width: 1000 }}>
        <SplitPane
          sessionId={sessionId}
          left={<div>left</div>}
          right={<div>right</div>}
        />
      </div>,
    );
    const separator = screen.getByRole("separator");
    const container = separator.parentElement;
    if (!container) {
      throw new Error("missing split container");
    }
    vi.spyOn(container, "getBoundingClientRect").mockReturnValue({
      width: 1000,
      height: 600,
      top: 0,
      left: 0,
      bottom: 600,
      right: 1000,
      x: 0,
      y: 0,
      toJSON: () => ({}),
    } as DOMRect);
    Object.defineProperty(separator, "setPointerCapture", {
      value: vi.fn(),
      configurable: true,
    });
    Object.defineProperty(separator, "releasePointerCapture", {
      value: vi.fn(),
      configurable: true,
    });
    return separator;
  }

  it("should change width on drag and save once on release when separator is dragged", () => {
    const separator = renderSplit(7);
    const setItem = vi.spyOn(window.localStorage, "setItem");

    fireEvent.pointerDown(separator, { clientX: 280, pointerId: 1 });
    fireEvent.pointerMove(separator, { clientX: 340, pointerId: 1 });
    fireEvent.pointerMove(separator, { clientX: 360, pointerId: 1 });
    expect(separator.getAttribute("aria-valuenow")).toBe("360");

    fireEvent.pointerUp(separator, { clientX: 360, pointerId: 1 });

    expect(separator.getAttribute("aria-valuenow")).toBe("360");
    const sessionWrites = setItem.mock.calls.filter(
      ([key]) => key === "tuki.sidebarWidth.7",
    );
    expect(sessionWrites).toHaveLength(1);
    expect(sessionWrites[0]?.[1]).toBe("360");
  });

  it("should increase and decrease by 16px with keyboard without leaving range when arrow keys pressed", () => {
    const separator = renderSplit(8);
    expect(separator.getAttribute("aria-valuenow")).toBe("280");

    fireEvent.keyDown(separator, { key: "ArrowRight" });
    expect(separator.getAttribute("aria-valuenow")).toBe("296");

    fireEvent.keyDown(separator, { key: "ArrowLeft" });
    fireEvent.keyDown(separator, { key: "ArrowLeft" });
    expect(separator.getAttribute("aria-valuenow")).toBe("264");

    for (let i = 0; i < 50; i += 1) {
      fireEvent.keyDown(separator, { key: "ArrowLeft" });
    }
    expect(separator.getAttribute("aria-valuenow")).toBe("180");

    for (let i = 0; i < 100; i += 1) {
      fireEvent.keyDown(separator, { key: "ArrowRight" });
    }
    expect(separator.getAttribute("aria-valuenow")).toBe("600");
  });
});
