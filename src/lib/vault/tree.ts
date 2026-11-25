import type { LinkReferenceDto, NoteSummaryDto, SyncStatusDto } from "../ipc/types";

export type DrawerView = "files" | "search" | "tags";
export type PanelTab = "backlinks" | "conflicts";
export type SyncIconName = "cloud" | "sync" | "alert" | "cloud-off";

export function getSyncUiMeta(syncStatus: SyncStatusDto): {
  iconName: SyncIconName;
  label: string;
  tooltip: string;
} {
  const iconName: SyncIconName =
    syncStatus.state === "synced"
      ? "cloud"
      : syncStatus.state === "syncing"
        ? "sync"
        : syncStatus.state === "error"
          ? "alert"
          : "cloud-off";

  const label =
    syncStatus.state === "synced"
      ? "Sincronizado"
      : syncStatus.state === "syncing"
        ? "Sincronizando…"
        : syncStatus.state === "error"
          ? "Error sinc."
          : syncStatus.state === "paused"
            ? "Pausado"
            : "Sin sincronizar";

  const tooltip =
    syncStatus.state === "synced"
      ? `Sincronizado con la nube${syncStatus.last_synced_at ? ` (${new Date(syncStatus.last_synced_at * 1000).toLocaleTimeString()})` : ""}`
      : syncStatus.state === "syncing"
        ? "Sincronizando cambios con la nube…"
        : syncStatus.state === "error"
          ? `Error de sincronización: ${syncStatus.last_error || "Desconocido"} (clic para ver)`
          : syncStatus.state === "paused"
            ? "Sincronización pausada"
            : "Sincronización desactivada (clic para configurar)";

  return { iconName, label, tooltip };
}

export type TreeContextTarget =
  | { kind: "note"; note: NoteSummaryDto; x: number; y: number }
  | { kind: "folder"; folderPath: string; folderName: string; x: number; y: number };

export type TreeActionModal =
  | { kind: "create-folder"; parentFolder?: string }
  | { kind: "rename-note"; note: NoteSummaryDto }
  | { kind: "repair-links"; note: NoteSummaryDto; newTitle: string; references: LinkReferenceDto[] }
  | { kind: "move-note"; note: NoteSummaryDto }
  | { kind: "delete-note"; note: NoteSummaryDto }
  | { kind: "rename-folder"; folderPath: string; folderName: string }
  | { kind: "delete-folder"; folderPath: string; folderName: string }
  | { kind: "move-folder-vault"; folderPath: string; folderName: string };

export function formatError(e: unknown): string {
  if (e == null) return "";
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  if (typeof e === "object") {
    const obj = e as Record<string, unknown>;
    if (typeof obj.message === "string") return obj.message;
    if (typeof obj.detail === "string") {
      return obj.code ? `${obj.code}: ${obj.detail}` : obj.detail;
    }
    if (typeof obj.error === "string") return obj.error;
    try {
      return JSON.stringify(e);
    } catch {
      return String(e);
    }
  }
  return String(e);
}

export function isHorizontallyScrollable(el: Element | null): boolean {
  let curr: Element | null = el;
  while (curr && curr !== document.body && curr !== document.documentElement) {
    if (
      curr.classList.contains("cm-table-wrap") ||
      curr.classList.contains("cm-codeblock-widget") ||
      curr.classList.contains("cm-mermaid-widget") ||
      curr.classList.contains("cm-math-block") ||
      curr.tagName === "TABLE" ||
      curr.tagName === "PRE"
    ) {
      return true;
    }
    try {
      const style = window.getComputedStyle(curr);
      const ox = style.overflowX;
      if ((ox === "auto" || ox === "scroll") && curr.scrollWidth > curr.clientWidth) {
        return true;
      }
    } catch {
      // ignore
    }
    curr = curr.parentElement;
  }
  return false;
}

export interface FolderNode {
  name: string;
  path: string;
  folders: FolderNode[];
  notes: NoteSummaryDto[];
}

export interface ConflictItem {
  noteId: string;
  title: string;
  path: string;
  localContent: string;
  diskContent: string;
  diskRev: number;
  detectedAt: string;
}

export function folderOf(path: string): string {
  const parts = path.split("/");
  if (parts.length <= 3) return "";
  return parts.slice(2, parts.length - 1).join("/");
}

export function buildTree(
  list: NoteSummaryDto[],
  folderPaths: string[],
  sortOrder: "az" | "modified" = "modified",
): FolderNode {
  const root: FolderNode = { name: "", path: "", folders: [], notes: [] };
  for (const folderPath of folderPaths) {
    const segs = folderPath.split("/").filter(Boolean);
    let node = root;
    for (const seg of segs) {
      let child = node.folders.find((c) => c.name === seg);
      if (!child) {
        const path = node.path ? `${node.path}/${seg}` : seg;
        child = { name: seg, path, folders: [], notes: [] };
        node.folders.push(child);
      }
      node = child;
    }
  }
  for (const note of list) {
    const segs = folderOf(note.path).split("/").filter(Boolean);
    let node = root;
    for (const seg of segs) {
      let child = node.folders.find((c) => c.name === seg);
      if (!child) {
        const path = node.path ? `${node.path}/${seg}` : seg;
        child = { name: seg, path, folders: [], notes: [] };
        node.folders.push(child);
      }
      node = child;
    }
    node.notes.push(note);
  }
  const sort = (node: FolderNode): void => {
    node.folders.sort((a, b) => a.name.localeCompare(b.name, "es"));
    node.notes.sort((a, b) =>
      sortOrder === "az"
        ? a.title.localeCompare(b.title, "es")
        : (b.updated_at ?? "").localeCompare(a.updated_at ?? ""),
    );
    node.folders.forEach(sort);
  };
  sort(root);
  return root;
}

export function collectFolderPaths(node: FolderNode, out: string[] = []): string[] {
  for (const f of node.folders) {
    out.push(f.path);
    collectFolderPaths(f, out);
  }
  return out;
}
