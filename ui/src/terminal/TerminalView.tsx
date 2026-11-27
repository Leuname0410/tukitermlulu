import "@xterm/xterm/css/xterm.css";
import { useEffect, useRef, useState, type JSX } from "react";
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

  useEffect(() => {
    if (menu === null) {
      return;
    }
    const closeOnEscape = (event: KeyboardEvent): void => {
      if (event.key === "Escape") {
        setMenu(null);
      }
    };
    const closeOnOutsideClick = (): void => {
      setMenu(null);
    };
    document.addEventListener("keydown", closeOnEscape);
    document.addEventListener("click", closeOnOutsideClick);
    return () => {
      document.removeEventListener("keydown", closeOnEscape);
      document.removeEventListener("click", closeOnOutsideClick);
    };
  }, [menu]);

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
        <div role="menu" style={{ position: "fixed", left: menu.x, top: menu.y }}>
          <button
            type="button"
            role="menuitem"
            onClick={() => {
              void copySelection();
              setMenu(null);
            }}
          >
            Copiar
          </button>
          <button
            type="button"
            role="menuitem"
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
