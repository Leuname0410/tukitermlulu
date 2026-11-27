import type { EntryDto } from "../lib/api";

export type SortKey = "name" | "size";

function compareNames(left: string, right: string): number {
  return left.localeCompare(right, undefined, { numeric: true });
}

/** Ordena entradas con las carpetas primero, según la clave y dirección dadas. */
export function sortEntries(
  entries: readonly EntryDto[],
  key: SortKey,
  ascending: boolean,
): EntryDto[] {
  const direction = ascending ? 1 : -1;
  return [...entries].sort((left, right) => {
    if (left.isDir !== right.isDir) {
      return left.isDir ? -1 : 1;
    }
    const primary =
      key === "size" ? left.size - right.size : compareNames(left.name, right.name);
    const comparison = primary === 0 ? compareNames(left.name, right.name) : primary;
    return comparison * direction;
  });
}

/** Devuelve el directorio padre; la raíz no tiene padre. */
export function parentPath(path: string): string | null {
  const normalized = path.length > 1 && path.endsWith("/") ? path.replace(/\/+$/, "") : path;
  const separator = normalized.lastIndexOf("/");
  if (separator < 0) {
    return null;
  }
  if (separator === 0) {
    return normalized.length > 1 ? "/" : null;
  }
  const parent = normalized.slice(0, separator);
  return /^\/[A-Za-z]:$/.test(parent) ? `${parent}/` : parent;
}

/** Compone una ruta hija sin duplicar el separador final. */
export function joinPath(directory: string, name: string): string {
  return directory.endsWith("/") ? `${directory}${name}` : `${directory}/${name}`;
}

/** Formatea tamaños en bytes con unidades binarias y un decimal. */
export function formatSize(bytes: number): string {
  if (bytes < 1024) {
    return `${bytes} B`;
  }
  const units = ["KiB", "MiB", "GiB", "TiB", "PiB"];
  let value = bytes / 1024;
  let index = 0;
  while (value >= 1024 && index < units.length - 1) {
    value /= 1024;
    index += 1;
  }
  return `${value.toFixed(1)} ${units[index] ?? "PiB"}`;
}
