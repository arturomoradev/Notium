// Editor-engine contract. CodeMirror 6 mounts imperatively into a host element
// and exposes this handle to the workspace shell.

import type { FormattingEngine } from "./toolbar";

export interface EditorEngineCallbacks {
  /** Called whenever the document changes (debounced save is owned by the shell). */
  onChange: () => void;
  /** Called when the user activates a wiki link (`[[..]]`). */
  onLinkActivate: (target: string) => void;
  /** Note title shown as an editable heading above the content. */
  title?: string;
  /** Renames the note from that heading; resolves false if it did not happen. */
  onRenameTitle?: (title: string) => Promise<boolean>;
}

export interface EditorEngine {
  /** Load content without emitting `onChange` (external load). */
  setContent(content: string): void;
  /** The canonical source-of-truth `.md` string. */
  getContent(): string;
  focus(): void;
  destroy(): void;
}

export interface EditorHandle {
  getContent(): string;
  getPlainText(): string;
  focus(): void;
  blur(): void;
  getEngine(): FormattingEngine | undefined;
  setViewMode?(mode: "edit" | "reading", sourceMode?: boolean): void;
  scrollToHeading?(heading: string): boolean;
}
