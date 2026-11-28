import type { EditorHandle } from "../../components/editors/engine";
import {
  COMMANDS_BY_ID,
  DEFAULT_TOOLBAR,
  type FormattingEngine,
  type ToolbarAction,
} from "../../components/editors/toolbar";

export type EditorKind = "tiptap";
export type ViewMode = "edit" | "reading";

// Per device, like Obsidian's mobile toolbar: a phone and a PC want different bars.
const TOOLBAR_KEY = "notium-toolbar";

function loadToolbar(): string[] {
  try {
    const ids: unknown = JSON.parse(localStorage.getItem(TOOLBAR_KEY) ?? "null");
    if (Array.isArray(ids)) return ids.filter((id) => COMMANDS_BY_ID.has(id));
  } catch {
    // Blocked or corrupt storage: fall back to the default bar.
  }
  return [...DEFAULT_TOOLBAR];
}

const FULL_WIDTH_KEY = "notium-full-width";

function applyFullWidth(on: boolean): void {
  document.documentElement.style.setProperty("--editor-max-width", on ? "none" : "800px");
}

function loadFullWidth(): boolean {
  let on = false;
  try {
    on = localStorage.getItem(FULL_WIDTH_KEY) === "1";
  } catch {
    // Blocked storage: default readable width.
  }
  applyFullWidth(on);
  return on;
}

class EditorStore {
  mounted = $state<{ format: EditorKind; handle: EditorHandle } | undefined>(undefined);
  viewMode = $state<ViewMode>("edit");
  toolbarIds = $state<string[]>(loadToolbar());
  toolbarConfigOpen = $state(false);
  fullWidth = $state(loadFullWidth());

  toggleFullWidth(): void {
    this.fullWidth = !this.fullWidth;
    applyFullWidth(this.fullWidth);
    try {
      localStorage.setItem(FULL_WIDTH_KEY, this.fullWidth ? "1" : "0");
    } catch {
      // Blocked storage: the choice lasts for this session only.
    }
  }

  current(activeFormat: EditorKind = "tiptap"): EditorHandle | undefined {
    return this.mounted?.format === activeFormat ? this.mounted.handle : undefined;
  }

  register(format: EditorKind, handle: EditorHandle, sourceMode: boolean): void {
    this.mounted = { format, handle };
    if (this.viewMode === "reading") handle.setViewMode?.(this.viewMode, sourceMode);
  }

  unregister(format: EditorKind, handle: EditorHandle): void {
    if (this.mounted?.format === format && this.mounted.handle === handle) {
      this.mounted = undefined;
    }
  }

  getEngine(): FormattingEngine | undefined {
    return this.mounted?.handle.getEngine();
  }

  setToolbar(ids: string[]): void {
    this.toolbarIds = ids;
    try {
      localStorage.setItem(TOOLBAR_KEY, JSON.stringify(ids));
    } catch {
      // Blocked storage: the order lasts for this session only.
    }
  }

  mainActions: ToolbarAction[] = $derived(
    this.toolbarIds.map((id) => {
      const c = COMMANDS_BY_ID.get(id)!;
      return {
        id,
        title: c.title,
        icon: c.icon,
        run: c.run
          ? () => {
              const e = this.getEngine();
              if (e) c.run!(e);
            }
          : () => {
              // Drop the editor focus so the keyboard does not cover the panel.
              (document.activeElement as HTMLElement | null)?.blur();
              this.toolbarConfigOpen = true;
            },
      };
    }),
  );
}

export const editorStore = new EditorStore();
