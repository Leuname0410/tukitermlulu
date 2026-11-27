import "@xterm/xterm/css/xterm.css";
import { useRef, useState, type JSX } from "react";
import { useTerminal, type UseTerminalOptions } from "./useTerminal";

interface MenuPosition {
  x: number;
  y: number;
}

/** Componente fino: el hook posee toda la lógica de xterm y este solo dibuja el menú contextual. */
export function TerminalView(props: UseTerminalOptions): JSX.Element {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const { copySelection, pasteClipboard } = useTerminal(containerRef, props);
  const [menu, setMenu] = useState<MenuPosition | null>(null);

  return (
    <div
      role="region"
      aria-label="Terminal"
      onContextMenu={(event) => {
        event.preventDefault();
        setMenu({ x: event.clientX, y: event.clientY });
      }}
    >
      <div ref={containerRef} className="terminal-view__xterm" />
      {menu !== null && (
        <div role="menu" style={{ left: menu.x, top: menu.y }}>
          <button
            type="button"
            onClick={() => {
              void copySelection();
              setMenu(null);
            }}
          >
            Copiar
          </button>
          <button
            type="button"
            onClick={() => {
              void pasteClipboard();
              setMenu(null);
            }}
          >
            Pegar
          </button>
        </div>
      )}
    </div>
  );
}
