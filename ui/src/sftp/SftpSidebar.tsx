import { useCallback, useEffect, useRef, useState, type JSX, type KeyboardEvent } from "react";
import type { EntryDto } from "../lib/api";
import { isCommandError, sftpHome, sftpList } from "../lib/api";
import { formatSize, joinPath, parentPath, sortEntries, type SortKey } from "./sidebarModel";

export interface SftpSidebarProps {
  connectionId: number;
  path?: string;
  onPathChange?(path: string): void;
}

interface SortState {
  key: SortKey;
  ascending: boolean;
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
  onPathChange,
}: SftpSidebarProps): JSX.Element {
  const [currentPath, setCurrentPath] = useState(path ?? "");
  const [routeInput, setRouteInput] = useState(path ?? "");
  const [entries, setEntries] = useState<readonly EntryDto[]>([]);
  const [sort, setSort] = useState<SortState>({ key: "name", ascending: true });
  const [busy, setBusy] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const requestRef = useRef(0);
  const pathChangeRef = useRef(onPathChange);

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
      void list(target, token);
    },
    [list],
  );

  useEffect(() => {
    const token = ++requestRef.current;
    void (async () => {
      setBusy(true);
      setError(null);
      try {
        const target = path ?? (await sftpHome(connectionId));
        if (token !== requestRef.current) {
          return;
        }
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
    <aside aria-label="Explorador SFTP" aria-busy={busy} style={{ width: 280 }}>
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
            >
              <td>
                {item.isDir ? "📁" : "📄"} <span>{item.name}</span>
              </td>
              <td>{item.isDir ? "" : formatSize(item.size)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </aside>
  );
}
