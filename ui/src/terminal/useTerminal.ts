import { FitAddon } from "@xterm/addon-fit";
import { WebglAddon } from "@xterm/addon-webgl";
import { Terminal } from "@xterm/xterm";
import { useCallback, useEffect, useRef, type RefObject } from "react";
import { terminalResize, terminalWrite } from "../lib/api";
import { clipboardAction, detectPlatform, type Platform } from "./shortcuts";

/** Fuente de bytes de salida del PTY; devuelve la función para desconectarse. */
export interface TerminalSource {
  attach(onData: (bytes: Uint8Array) => void): () => void;
}

export interface UseTerminalOptions {
  channelId: number;
  source: TerminalSource;
  platform?: Platform;
}

export interface TerminalControls {
  copySelection(): Promise<void>;
  pasteClipboard(): Promise<void>;
}

const encoder = new TextEncoder();

/** Monta xterm, conecta el source y gestiona E/S, portapapeles y redimensionado. */
export function useTerminal(
  container: RefObject<HTMLDivElement | null>,
  options: UseTerminalOptions,
): TerminalControls {
  const { channelId, source } = options;
  const platform = options.platform ?? detectPlatform(navigator.userAgent);
  const terminalRef = useRef<Terminal | null>(null);

  const copySelection = useCallback(async (): Promise<void> => {
    const terminal = terminalRef.current;
    if (!terminal) {
      return;
    }
    try {
      await navigator.clipboard.writeText(terminal.getSelection());
    } catch {
      // Un fallo del portapapeles no debe romper la terminal.
    }
  }, []);

  const pasteClipboard = useCallback(async (): Promise<void> => {
    const terminal = terminalRef.current;
    if (!terminal) {
      return;
    }
    try {
      const text = await navigator.clipboard.readText();
      terminal.paste(text);
    } catch {
      // Un fallo del portapapeles no debe romper la terminal.
    }
  }, []);

  useEffect(() => {
    const element = container.current;
    if (!element) {
      return;
    }

    const terminal = new Terminal();
    terminalRef.current = terminal;
    terminal.open(element);

    const fitAddon = new FitAddon();
    terminal.loadAddon(fitAddon);
    fitAddon.fit();
    // El PTY arranca con el tamaño por defecto; sincroniza el tamaño real tras el fit inicial.
    void terminalResize(channelId, terminal.rows, terminal.cols);

    // WebGL es opcional: si falla o pierde el contexto seguimos con el renderer por defecto.
    let webglAddon: WebglAddon | null = null;
    let candidate: WebglAddon | null = null;
    try {
      candidate = new WebglAddon();
      candidate.onContextLoss(() => {
        candidate?.dispose();
        if (webglAddon === candidate) {
          webglAddon = null;
        }
      });
      terminal.loadAddon(candidate);
      webglAddon = candidate;
    } catch {
      candidate?.dispose();
      webglAddon = null;
    }

    const detach = source.attach((bytes) => {
      terminal.write(bytes);
    });

    terminal.onData((data) => {
      void terminalWrite(channelId, encoder.encode(data));
    });
    terminal.onBinary((data) => {
      void terminalWrite(channelId, encoder.encode(data));
    });

    terminal.attachCustomKeyEventHandler((event) => {
      const action = clipboardAction(event, platform);
      if (action === "copy") {
        // Evita que el navegador maneje el atajo además del hook (doble pegado/copia).
        event.preventDefault();
        void copySelection();
        return false;
      }
      if (action === "paste") {
        event.preventDefault();
        void pasteClipboard();
        return false;
      }
      return true;
    });

    let lastRows = terminal.rows;
    let lastCols = terminal.cols;
    let debounce: ReturnType<typeof setTimeout> | null = null;
    const observer = new ResizeObserver(() => {
      fitAddon.fit();
      if (debounce !== null) {
        clearTimeout(debounce);
      }
      debounce = setTimeout(() => {
        debounce = null;
        if (terminal.rows === lastRows && terminal.cols === lastCols) {
          return;
        }
        lastRows = terminal.rows;
        lastCols = terminal.cols;
        void terminalResize(channelId, terminal.rows, terminal.cols);
      }, 100);
    });
    observer.observe(element);

    return () => {
      observer.disconnect();
      if (debounce !== null) {
        clearTimeout(debounce);
      }
      detach();
      webglAddon?.dispose();
      fitAddon.dispose();
      terminal.dispose();
      terminalRef.current = null;
    };
  }, [container, channelId, source, platform, copySelection, pasteClipboard]);

  return { copySelection, pasteClipboard };
}
