import { useMemo, useState, type JSX } from "react";
import type { FolderDto, FolderInput, SessionDto } from "../lib/api";
import { buildTree, filterTree, type TreeNode } from "./sessionTree";

interface TreeActions {
  onConnect(session: SessionDto): void;
  onEdit(session: SessionDto): void;
  onDuplicate(session: SessionDto): void;
  onDelete(session: SessionDto): void;
  onSaveFolder(folder: FolderInput): void;
  onDeleteFolder(id: number): void;
}

export interface SessionTreeProps extends TreeActions {
  folders: readonly FolderDto[];
  sessions: readonly SessionDto[];
  onNewSession(): void;
}

interface FolderNodeProps extends TreeActions {
  folder: FolderDto;
  folders: readonly TreeNode[];
  sessions: readonly SessionDto[];
  collapsed: ReadonlySet<number>;
  searchActive: boolean;
  onToggle(id: number): void;
  onNewFolder(parent: number | null): void;
  onRename(folder: FolderDto): void;
}

function FolderNode(props: FolderNodeProps): JSX.Element {
  const expanded = props.searchActive || !props.collapsed.has(props.folder.id);
  return (
    <div
      role="treeitem"
      aria-label={`Carpeta ${props.folder.name}`}
      aria-expanded={expanded}
    >
      <span>{props.folder.name}</span>
      <button type="button" aria-label={`${expanded ? "Plegar" : "Desplegar"} ${props.folder.name}`} onClick={() => props.onToggle(props.folder.id)}>
        {expanded ? "−" : "+"}
      </button>
      <button type="button" aria-label={`Nueva carpeta en ${props.folder.name}`} onClick={() => props.onNewFolder(props.folder.id)}>
        Nueva carpeta
      </button>
      <button type="button" aria-label={`Renombrar ${props.folder.name}`} onClick={() => props.onRename(props.folder)}>
        Renombrar
      </button>
      <button type="button" aria-label={`Borrar carpeta ${props.folder.name}`} onClick={() => props.onDeleteFolder(props.folder.id)}>
        Borrar
      </button>
      {expanded && (
        <div role="group">
          {props.folders.map((node) =>
            node.folder === null ? null : (
              <FolderNode
                key={node.folder.id}
                {...props}
                folder={node.folder}
                folders={node.folders}
                sessions={node.sessions}
              />
            ),
          )}
          {props.sessions.map((session) => (
            <SessionNode key={session.id} {...props} session={session} />
          ))}
        </div>
      )}
    </div>
  );
}

interface SessionNodeProps extends TreeActions {
  session: SessionDto;
}

function SessionNode(props: SessionNodeProps): JSX.Element {
  const session = props.session;
  return (
    <div
      role="treeitem"
      aria-label={`Sesión ${session.name}`}
      tabIndex={0}
      onDoubleClick={() => props.onConnect(session)}
      onKeyDown={(event) => {
        if (event.key === "Enter" && event.target === event.currentTarget) {
          props.onConnect(session);
        }
      }}
    >
      <span>{session.name}</span>
      <button type="button" aria-label={`Conectar ${session.name}`} onClick={() => props.onConnect(session)}>
        Conectar
      </button>
      <button type="button" aria-label={`Editar sesión ${session.name}`} onClick={() => props.onEdit(session)}>
        Editar
      </button>
      <button type="button" aria-label={`Duplicar sesión ${session.name}`} onClick={() => props.onDuplicate(session)}>
        Duplicar
      </button>
      <button
        type="button"
        aria-label={`Borrar sesión ${session.name}`}
        onClick={() => {
          if (window.confirm(`¿Borrar la sesión ${session.name}?`)) {
            props.onDelete(session);
          }
        }}
      >
        Borrar
      </button>
    </div>
  );
}

/** Barra lateral con árbol, búsqueda y acciones sobre sesiones y carpetas. */
export function SessionTree(props: SessionTreeProps): JSX.Element {
  const [query, setQuery] = useState("");
  const [collapsed, setCollapsed] = useState<ReadonlySet<number>>(new Set());
  const tree = useMemo(
    () => filterTree(buildTree(props.folders, props.sessions), query),
    [props.folders, props.sessions, query],
  );
  const searchActive = query.trim() !== "";

  function toggle(id: number): void {
    setCollapsed((previous) => {
      const next = new Set(previous);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  }

  function newFolder(parent: number | null): void {
    const name = window.prompt("Nombre de la carpeta");
    if (name !== null && name.trim() !== "") {
      props.onSaveFolder({ name: name.trim(), parent });
    }
  }

  function renameFolder(folder: FolderDto): void {
    const name = window.prompt("Renombrar carpeta", folder.name);
    if (name !== null && name.trim() !== "") {
      props.onSaveFolder({ id: folder.id, name: name.trim(), parent: folder.parent });
    }
  }

  function deleteFolder(id: number): void {
    const folder = props.folders.find((item) => item.id === id);
    if (folder !== undefined && window.confirm(`¿Borrar la carpeta ${folder.name}?`)) {
      props.onDeleteFolder(id);
    }
  }

  return (
    <section aria-label="Gestor de sesiones">
      <button type="button" aria-label="Nueva sesión" onClick={props.onNewSession}>
        Nueva sesión
      </button>
      <button type="button" aria-label="Nueva carpeta" onClick={() => newFolder(null)}>
        Nueva carpeta
      </button>
      <input
        type="search"
        aria-label="Buscar sesiones"
        value={query}
        onChange={(event) => setQuery(event.target.value)}
      />
      <div role="tree" aria-label="Sesiones">
        {tree.folders.map((node) =>
          node.folder === null ? null : (
            <FolderNode
              key={node.folder.id}
              {...props}
              folder={node.folder}
              folders={node.folders}
              sessions={node.sessions}
              collapsed={collapsed}
              searchActive={searchActive}
              onToggle={toggle}
              onNewFolder={newFolder}
              onRename={renameFolder}
              onDeleteFolder={deleteFolder}
            />
          ),
        )}
        {tree.sessions.map((session) => (
          <SessionNode key={session.id} {...props} session={session} />
        ))}
      </div>
    </section>
  );
}
