import { useCallback, useEffect, useRef, useState, type JSX, type KeyboardEvent } from "react";
import type { EntryDto, TransferDto } from "../lib/api";
import { isCommandError, onTransfers, sftpHome, sftpList } from "../lib/api";
import { EntryMenu } from "./EntryMenu";
import { formatSize, joinPath, parentPath, sortEntries, type SortKey } from "./sidebarModel";
import { TransferList } from "./TransferList";
import { useFileDrop } from "./useFileDrop";

export interface SftpSidebarProps {
  connectionId: number;
  path?: string;
  followPath?: string;
  onPathChange?(path: string): void;
}

interface SortState {
  key: SortKey;
  ascending: boolean;
}

interface MenuState {
  x: number;
  y: number;
  entry?: EntryDto;
}

function errorText(cause: unknown): string {
  if (isCommandError(cause)) {
    return cause.message;
  }
  return cause instanceof Error ? cause.message : "No se pudo listar el directorio remoto";
}

/** Explorador SFTP: lista el directorio actual y permite navegar por él. */
export function SftpSidebar({
  connectionId,
  path,
  followPath,
  onPathChange,
}: SftpSidebarProps): JSX.Element {
  const [currentPath, setCurrentPath] = useState(path ?? "");
  const [routeInput, setRouteInput] = useState(path ?? "");
  const [entries, setEntries] = useState<readonly EntryDto[]>([]);
  const [sort, setSort] = useState<SortState>({ key: "name", ascending: true });
  const [busy, setBusy] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [followTerminal, setFollowTerminal] = useState(true);
  const [menu, setMenu] = useState<MenuState | null>(null);
  const [transfers, setTransfers] = useState<readonly TransferDto[]>([]);
  const requestRef = useRef(0);
  const pathChangeRef = useRef(onPathChange);
  const sidebarRef = useRef<HTMLElement>(null);
  const currentPathRef = useRef("");
  const lastFollowRef = useRef(followPath);

  useEffect(() => {
    pathChangeRef.current = onPathChange;
  }, [onPathChange]);

  const list = useCallback(
    async (target: string, token: number): Promise<void> => {
      try {
        const listed = await sftpList(connectionId, target);
        if (token !== requestRef.current) {
          return;
        }
        setEntries(listed);
        currentPathRef.current = target;
        setCurrentPath(target);
        setRouteInput(target);
        pathChangeRef.current?.(target);
      } catch (cause) {
        if (token === requestRef.current) {
          setError(errorText(cause));
        }
      } finally {
        if (token === requestRef.current) {
          setBusy(false);
        }
      }
    },
    [connectionId],
  );

  const navigate = useCallback(
    (target: string): void => {
      const token = ++requestRef.current;
      setBusy(true);
      setError(null);
      setMenu(null);
      void list(target, token);
    },
    [list],
  );

  const dropActive = useFileDrop({
    connectionId,
    currentPath,
    sidebarRef,
    onError: setError,
    onRefresh: () => navigate(currentPath),
  });

  useEffect(() => {
    let active = true;
    let stop: (() => void) | undefined;
    void (async () => {
      try {
        const unsubscribe = await onTransfers((jobs) => {
          if (active) setTransfers(jobs);
        });
        if (active) {
          stop = unsubscribe;
        } else {
          unsubscribe();
        }
      } catch {
        // Sin webview Tauri no hay eventos de transferencia.
      }
    })();
    return () => {
      active = false;
      stop?.();
    };
  }, []);

  useEffect(() => {
    const token = ++requestRef.current;
    void (async () => {
      setBusy(true);
      setError(null);
      try {
        const target = path ?? lastFollowRef.current ?? (await sftpHome(connectionId));
        if (token !== requestRef.current) {
          return;
        }
        currentPathRef.current = target;
        setCurrentPath(target);
        setRouteInput(target);
        await list(target, token);
      } catch (cause) {
        if (token === requestRef.current) {
          setError(errorText(cause));
          setBusy(false);
        }
      }
    })();
    return () => {
      requestRef.current += 1;
    };
  }, [connectionId, path, list]);

  useEffect(() => {
    if (followPath === undefined) {
      return;
    }
    const isNew = followPath !== lastFollowRef.current;
    lastFollowRef.current = followPath;
    if (isNew && followTerminal && followPath !== currentPathRef.current) {
      navigate(followPath);
    }
  }, [followPath, followTerminal, navigate]);

  function toggleSort(key: SortKey): void {
    setSort((previous) =>
      previous.key === key
        ? { key, ascending: !previous.ascending }
        : { key, ascending: true },
    );
  }

  function openEntry(item: EntryDto): void {
    if (item.isDir) {
      navigate(joinPath(currentPath, item.name));
    }
  }

  function onRowKeyDown(event: KeyboardEvent<HTMLTableRowElement>, item: EntryDto): void {
    if (event.key === "Enter") {
      openEntry(item);
    }
  }

  const parent = parentPath(currentPath);
  const sorted = sortEntries(entries, sort.key, sort.ascending);

  return (
    <aside
      ref={sidebarRef}
      aria-label="Explorador SFTP"
      aria-busy={busy}
      data-drop-active={dropActive ? "true" : "false"}
      style={{ width: 280 }}
      onContextMenu={(event) => {
        event.preventDefault();
        setMenu({ x: event.clientX, y: event.clientY });
      }}
    >
      {dropActive && <p role="status">Soltar para subir</p>}
      <div>
        <input
          aria-label="Ruta remota"
          value={routeInput}
          onChange={(event) => setRouteInput(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              navigate(routeInput);
            }
          }}
        />
        <button
          type="button"
          aria-label="Subir de nivel"
          disabled={parent === null}
          onClick={() => {
            if (parent !== null) {
              navigate(parent);
            }
          }}
        >
          ↑
        </button>
        <button type="button" aria-label="Refrescar" onClick={() => navigate(currentPath)}>
          ↻
        </button>
      </div>
      <label>
        <input
          type="checkbox"
          checked={followTerminal}
          onChange={(event) => {
            const checked = event.target.checked;
            setFollowTerminal(checked);
            const latest = lastFollowRef.current;
            if (checked && latest !== undefined && latest !== currentPathRef.current) {
              navigate(latest);
            }
          }}
        />
        Seguir terminal
      </label>
      {error !== null && <p role="alert">{error}</p>}
      <table role="grid" aria-label="Entradas remotas">
        <thead>
          <tr>
            <th aria-sort={sort.key === "name" ? (sort.ascending ? "ascending" : "descending") : "none"}>
              <button type="button" onClick={() => toggleSort("name")}>
                Nombre
              </button>
            </th>
            <th aria-sort={sort.key === "size" ? (sort.ascending ? "ascending" : "descending") : "none"}>
              <button type="button" onClick={() => toggleSort("size")}>
                Tamaño
              </button>
            </th>
          </tr>
        </thead>
        <tbody>
          {sorted.map((item) => (
            <tr
              key={item.path}
              tabIndex={0}
              onDoubleClick={() => openEntry(item)}
              onKeyDown={(event) => onRowKeyDown(event, item)}
              onContextMenu={(event) => {
                event.preventDefault();
                event.stopPropagation();
                setMenu({ x: event.clientX, y: event.clientY, entry: item });
              }}
            >
              <td>
                {item.isDir ? "📁" : "📄"} <span>{item.name}</span>
              </td>
              <td>{item.isDir ? "" : formatSize(item.size)}</td>
            </tr>
          ))}
        </tbody>
      </table>
      <TransferList transfers={transfers} onError={setError} />
      {menu !== null && (
        <EntryMenu
          connectionId={connectionId}
          directory={currentPath}
          entry={menu.entry}
          x={menu.x}
          y={menu.y}
          onClose={() => setMenu(null)}
          onRefresh={() => navigate(currentPath)}
          onError={setError}
        />
      )}
    </aside>
  );
}
