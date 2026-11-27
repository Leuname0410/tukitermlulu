import type { FolderDto, SessionDto } from "../lib/api";
import type { ConnectTarget } from "../tabs/tabState";

/** Nodo del árbol de sesiones; la raíz tiene `folder` a null. */
export interface TreeNode {
  folder: FolderDto | null;
  folders: TreeNode[];
  sessions: SessionDto[];
}

function byName<T extends { name: string }>(left: T | null, right: T | null): number {
  return (left?.name ?? "").localeCompare(right?.name ?? "", "es");
}

function sortNode(node: TreeNode, visited: ReadonlySet<TreeNode>): void {
  if (visited.has(node)) {
    return;
  }
  const seen = new Set(visited);
  seen.add(node);
  node.folders.sort((left, right) => byName(left.folder, right.folder));
  node.sessions.sort(byName);
  for (const child of node.folders) {
    sortNode(child, seen);
  }
}

/** Construye el árbol; los padres inexistentes y las sesiones sin carpeta van a la raíz. */
export function buildTree(folders: readonly FolderDto[], sessions: readonly SessionDto[]): TreeNode {
  const root: TreeNode = { folder: null, folders: [], sessions: [] };
  const nodes = new Map<number, TreeNode>();

  for (const folder of folders) {
    nodes.set(folder.id, { folder, folders: [], sessions: [] });
  }
  for (const folder of folders) {
    const node = nodes.get(folder.id);
    if (node === undefined) {
      continue;
    }
    const parent = folder.parent === null ? null : nodes.get(folder.parent);
    (parent ?? root).folders.push(node);
  }
  for (const session of sessions) {
    const parent = session.folder === null ? null : nodes.get(session.folder);
    (parent ?? root).sessions.push(session);
  }

  sortNode(root, new Set());
  return root;
}

function normalize(value: string): string {
  return value.normalize("NFD").replace(/[\u0300-\u036f]/g, "").toLocaleLowerCase("es");
}

function matchesSession(session: SessionDto, term: string): boolean {
  return [session.name, session.host, session.username].some((value) =>
    normalize(value).includes(term),
  );
}

function filterNode(node: TreeNode, term: string): TreeNode | null {
  const folderMatches = node.folder !== null && normalize(node.folder.name).includes(term);
  const folders = node.folders
    .map((child) => filterNode(child, term))
    .filter((child): child is TreeNode => child !== null);
  const sessions = folderMatches ? [...node.sessions] : node.sessions.filter((session) => matchesSession(session, term));

  if (node.folder === null || folderMatches || folders.length > 0 || sessions.length > 0) {
    return { folder: node.folder, folders, sessions };
  }
  return null;
}

/** Filtra por nombre, host o usuario sin distinción de mayúsculas ni acentos. */
export function filterTree(tree: TreeNode, query: string): TreeNode {
  const term = normalize(query.trim());
  return term === "" ? tree : (filterNode(tree, term) ?? { folder: null, folders: [], sessions: [] });
}

/** Construye el destino de conexión usando el id de sesión para recuperar secretos. */
export function sessionTarget(session: SessionDto): ConnectTarget {
  return {
    sessionId: session.id,
    host: session.host,
    port: session.port,
    username: session.username,
    auth: session.auth,
  };
}
