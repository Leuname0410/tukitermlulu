import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { TerminalView } from "./TerminalView";
import type { TerminalSource } from "./useTerminal";
import {
  clipboardAction,
  detectPlatform,
  type ClipboardAction,
  type Platform,
} from "./shortcuts";

interface MockTerminalState {
  writes: Array<Uint8Array | string>;
  pasted: string[];
  dataListeners: Array<(data: string) => void>;
  binaryListeners: Array<(data: string) => void>;
  keyHandler: ((event: KeyboardEvent) => boolean) | null;
  selection: string;
  rows: number;
  cols: number;
  disposed: boolean;
  addons: string[];
}

const mocks = vi.hoisted(() => {
  const state = {
    terminals: [] as MockTerminalState[],
    fitInstances: [] as Array<{ terminal: MockTerminalState | null; disposed: boolean }>,
    webglInstances: [] as Array<{
      disposed: boolean;
      contextLossHandlers: Array<() => void>;
    }>,
    observers: [] as Array<{ callback: () => void; disconnected: boolean }>,
    webglShouldThrow: false,
    size: { rows: 24, cols: 80 },
  };

  class FakeTerminal {
    writes: Array<Uint8Array | string> = [];
    pasted: string[] = [];
    dataListeners: Array<(data: string) => void> = [];
    binaryListeners: Array<(data: string) => void> = [];
    keyHandler: ((event: KeyboardEvent) => boolean) | null = null;
    selection = "";
    rows = 24;
    cols = 80;
    disposed = false;
    addons: string[] = [];

    constructor() {
      state.terminals.push(this);
    }

    open(): void {}
    write(data: string | Uint8Array): void {
      this.writes.push(data);
    }
    onData(listener: (data: string) => void): { dispose(): void } {
      this.dataListeners.push(listener);
      return { dispose: () => {} };
    }
    onBinary(listener: (data: string) => void): { dispose(): void } {
      this.binaryListeners.push(listener);
      return { dispose: () => {} };
    }
    attachCustomKeyEventHandler(handler: (event: KeyboardEvent) => boolean): void {
      this.keyHandler = handler;
    }
    loadAddon(addon: { name: string; activate(terminal: FakeTerminal): void }): void {
      this.addons.push(addon.name);
      addon.activate(this);
    }
    getSelection(): string {
      return this.selection;
    }
    paste(text: string): void {
      this.pasted.push(text);
    }
    dispose(): void {
      this.disposed = true;
    }
  }

  class FakeFitAddon {
    readonly name = "fit";
    terminal: FakeTerminal | null = null;
    disposed = false;

    constructor() {
      state.fitInstances.push(this);
    }
    activate(terminal: FakeTerminal): void {
      this.terminal = terminal;
    }
    dispose(): void {
      this.disposed = true;
    }
    fit(): void {
      if (this.terminal) {
        this.terminal.rows = state.size.rows;
        this.terminal.cols = state.size.cols;
      }
    }
  }

  class FakeWebglAddon {
    readonly name = "webgl";
    disposed = false;
    contextLossHandlers: Array<() => void> = [];

    constructor() {
      if (state.webglShouldThrow) {
        throw new Error("WebGL no disponible");
      }
      state.webglInstances.push(this);
    }
    onContextLoss(handler: () => void): { dispose(): void } {
      this.contextLossHandlers.push(handler);
      return { dispose: () => {} };
    }
    activate(): void {}
    dispose(): void {
      this.disposed = true;
    }
  }

  class FakeResizeObserver {
    callback: () => void;
    disconnected = false;

    constructor(callback: () => void) {
      this.callback = callback;
      state.observers.push(this);
    }
    observe(): void {}
    disconnect(): void {
      this.disconnected = true;
    }
  }

  return Object.assign(state, { FakeTerminal, FakeFitAddon, FakeWebglAddon, FakeResizeObserver });
});

const apiMocks = vi.hoisted(() => ({
  writes: [] as Array<{ channelId: number; data: Uint8Array }>,
  resizes: [] as Array<{ channelId: number; rows: number; cols: number }>,
}));

vi.mock("@xterm/xterm", () => ({ Terminal: mocks.FakeTerminal }));
vi.mock("@xterm/addon-fit", () => ({ FitAddon: mocks.FakeFitAddon }));
vi.mock("@xterm/addon-webgl", () => ({ WebglAddon: mocks.FakeWebglAddon }));
vi.mock("../lib/api", () => ({
  terminalWrite: async (channelId: number, data: Uint8Array): Promise<void> => {
    apiMocks.writes.push({ channelId, data });
  },
  terminalResize: async (channelId: number, rows: number, cols: number): Promise<void> => {
    apiMocks.resizes.push({ channelId, rows, cols });
  },
}));

const silentSource: TerminalSource = { attach: () => () => {} };

function lastTerminal(): MockTerminalState {
  const terminal = mocks.terminals.at(-1);
  if (!terminal) {
    throw new Error("No se creó la terminal");
  }
  return terminal;
}

function required<T>(value: T | undefined, message: string): T {
  if (value === undefined) {
    throw new Error(message);
  }
  return value;
}

beforeEach(() => {
  mocks.terminals.length = 0;
  mocks.fitInstances.length = 0;
  mocks.webglInstances.length = 0;
  mocks.observers.length = 0;
  mocks.webglShouldThrow = false;
  mocks.size = { rows: 24, cols: 80 };
  apiMocks.writes.length = 0;
  apiMocks.resizes.length = 0;
  vi.stubGlobal("ResizeObserver", mocks.FakeResizeObserver);
  Object.defineProperty(navigator, "clipboard", {
    configurable: true,
    value: {
      writeText: vi.fn().mockResolvedValue(undefined),
      readText: vi.fn().mockResolvedValue("texto del portapapeles"),
    },
  });
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe("detectPlatform", () => {
  it("should return mac when the user agent contains Mac", () => {
    expect(detectPlatform("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)")).toBe("mac");
  });

  it("should return other when the user agent does not contain Mac", () => {
    expect(detectPlatform("Mozilla/5.0 (Windows NT 10.0; Win64; x64)")).toBe("other");
  });
});

describe("clipboardAction", () => {
  const cases: Array<{
    description: string;
    event: KeyboardEvent;
    platform: Platform;
    expected: ClipboardAction;
  }> = [
    {
      description: "copy with Cmd+C on mac",
      event: new KeyboardEvent("keydown", { key: "c", metaKey: true }),
      platform: "mac",
      expected: "copy",
    },
    {
      description: "paste with Cmd+V on mac",
      event: new KeyboardEvent("keydown", { key: "v", metaKey: true }),
      platform: "mac",
      expected: "paste",
    },
    {
      description: "copy with Ctrl+Shift+C on other platforms",
      event: new KeyboardEvent("keydown", { key: "C", ctrlKey: true, shiftKey: true }),
      platform: "other",
      expected: "copy",
    },
    {
      description: "paste with Ctrl+Shift+V on other platforms",
      event: new KeyboardEvent("keydown", { key: "V", ctrlKey: true, shiftKey: true }),
      platform: "other",
      expected: "paste",
    },
    {
      description: "null for Ctrl+C without Shift on other platforms",
      event: new KeyboardEvent("keydown", { key: "c", ctrlKey: true }),
      platform: "other",
      expected: null,
    },
    {
      description: "null for Cmd+C with Ctrl on mac",
      event: new KeyboardEvent("keydown", { key: "c", metaKey: true, ctrlKey: true }),
      platform: "mac",
      expected: null,
    },
    {
      description: "null on keyup",
      event: new KeyboardEvent("keyup", { key: "c", metaKey: true }),
      platform: "mac",
      expected: null,
    },
  ];

  it.each(cases)("should $description", ({ event, platform, expected }) => {
    expect(clipboardAction(event, platform)).toBe(expected);
  });
});

describe("TerminalView", () => {
  it("should write source bytes verbatim into the terminal", () => {
    const attached: { handler: ((bytes: Uint8Array) => void) | null } = { handler: null };
    const source: TerminalSource = {
      attach: (onData) => {
        attached.handler = onData;
        return () => {};
      },
    };
    render(<TerminalView channelId={7} source={source} platform="other" />);

    const bytes = new Uint8Array([27, 91, 51, 49, 109]);
    const handler = attached.handler;
    if (!handler) {
      throw new Error("source.attach no recibió el handler");
    }
    handler(bytes);
    expect(lastTerminal().writes[0]).toBe(bytes);
  });

  it("should send onData text as encoded bytes to terminalWrite", async () => {
    render(<TerminalView channelId={7} source={silentSource} platform="other" />);
    const listeners = lastTerminal().dataListeners;
    expect(listeners).toHaveLength(1);

    const listener = required(listeners[0], "No se registró onData");
    await act(async () => {
      listener("ls\r");
    });

    expect(apiMocks.writes).toHaveLength(1);
    const write = required(apiMocks.writes[0], "No se llamó terminalWrite");
    expect(write.channelId).toBe(7);
    expect(new TextDecoder().decode(write.data)).toBe("ls\r");
  });

  it("should copy the selection and skip terminalWrite when the shortcut is pressed", async () => {
    render(<TerminalView channelId={7} source={silentSource} platform="mac" />);
    const terminal = lastTerminal();
    terminal.selection = "texto seleccionado";
    const handler = terminal.keyHandler;
    if (!handler) {
      throw new Error("No se registró el manejador de teclas");
    }

    let handled = true;
    await act(async () => {
      handled = handler(new KeyboardEvent("keydown", { key: "c", metaKey: true }));
    });

    expect(handled).toBe(false);
    expect(vi.mocked(navigator.clipboard.writeText)).toHaveBeenCalledWith("texto seleccionado");
    expect(apiMocks.writes).toHaveLength(0);
  });

  it("should paste clipboard text with the shortcut or the menu button", async () => {
    render(<TerminalView channelId={7} source={silentSource} platform="other" />);
    const terminal = lastTerminal();
    const handler = terminal.keyHandler;
    if (!handler) {
      throw new Error("No se registró el manejador de teclas");
    }

    await act(async () => {
      handler(new KeyboardEvent("keydown", { key: "v", ctrlKey: true, shiftKey: true }));
    });
    await waitFor(() => {
      expect(terminal.pasted).toEqual(["texto del portapapeles"]);
    });

    fireEvent.contextMenu(screen.getByRole("region", { name: "Terminal" }));
    fireEvent.click(await screen.findByRole("menuitem", { name: "Pegar" }));
    await waitFor(() => {
      expect(terminal.pasted).toEqual([
        "texto del portapapeles",
        "texto del portapapeles",
      ]);
    });
  });

  it("should mount the terminal when the WebglAddon constructor throws", () => {
    mocks.webglShouldThrow = true;
    render(<TerminalView channelId={7} source={silentSource} platform="other" />);

    expect(screen.getByRole("region", { name: "Terminal" })).toBeTruthy();
    expect(lastTerminal().addons).toContain("fit");
    expect(lastTerminal().addons).not.toContain("webgl");
  });

  it("should call terminalResize once after the debounce when the size changes", async () => {
    vi.useFakeTimers();
    render(<TerminalView channelId={7} source={silentSource} platform="other" />);
    const observer = required(mocks.observers[0], "No se creó ResizeObserver");

    mocks.size = { rows: 40, cols: 120 };
    await act(async () => {
      observer.callback();
    });
    expect(apiMocks.resizes).toHaveLength(0);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(100);
    });
    expect(apiMocks.resizes).toEqual([{ channelId: 7, rows: 40, cols: 120 }]);

    await act(async () => {
      observer.callback();
      await vi.advanceTimersByTimeAsync(100);
    });
    expect(apiMocks.resizes).toHaveLength(1);
  });

  it.each(["mac", "other"] as const)(
    "should prevent the native clipboard event when the shortcut is pressed on %s",
    async (platform) => {
      render(<TerminalView channelId={7} source={silentSource} platform={platform} />);
      const registered = lastTerminal().keyHandler;
      if (!registered) {
        throw new Error("No se registró el manejador de teclas");
      }
      const handler = registered;
      const modifiers = platform === "mac" ? { metaKey: true } : { ctrlKey: true, shiftKey: true };

      for (const key of ["c", "v"]) {
        const event = new KeyboardEvent("keydown", { key, ...modifiers });
        const preventDefault = vi.spyOn(event, "preventDefault");
        await act(async () => {
          handler(event);
        });
        expect(preventDefault).toHaveBeenCalledTimes(1);
      }
      expect(apiMocks.writes).toHaveLength(0);
    },
  );

  it("should show a fixed context menu with menu items closed by Escape and outside click", async () => {
    render(<TerminalView channelId={7} source={silentSource} platform="other" />);
    fireEvent.contextMenu(screen.getByRole("region", { name: "Terminal" }), {
      clientX: 42,
      clientY: 84,
    });

    const menu = screen.getByRole("menu");
    expect(menu.style.position).toBe("fixed");
    expect(menu.style.left).toBe("42px");
    expect(menu.style.top).toBe("84px");
    expect(screen.getAllByRole("menuitem").map((item) => item.textContent)).toEqual([
      "Copiar",
      "Pegar",
    ]);

    fireEvent.keyDown(document, { key: "Escape" });
    expect(screen.queryByRole("menu")).toBeNull();

    fireEvent.contextMenu(screen.getByRole("region", { name: "Terminal" }));
    fireEvent.click(document.body);
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("should detach and dispose on unmount", () => {
    const detach = vi.fn();
    const source: TerminalSource = { attach: () => detach };
    const { unmount } = render(
      <TerminalView channelId={7} source={source} platform="other" />,
    );
    const terminal = lastTerminal();

    unmount();

    expect(detach).toHaveBeenCalledTimes(1);
    expect(terminal.disposed).toBe(true);
    expect(required(mocks.fitInstances[0], "No se creó FitAddon").disposed).toBe(true);
    expect(required(mocks.observers[0], "No se creó ResizeObserver").disconnected).toBe(true);
  });
});
