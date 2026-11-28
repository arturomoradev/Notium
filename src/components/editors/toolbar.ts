// Formatting-toolbar contract for the note editor (Tiptap).
import type { Editor } from "@tiptap/core";
import { TextSelection } from "@tiptap/pm/state";

export type ViewCommand = (editor: Editor) => boolean | void;

export interface FormattingEngine {
  undo(): void;
  redo(): void;
  noteLink(): void;
  embed(): void;
  tag(): void;
  attachment(): void;
  headingCycle(): void;
  bold(): void;
  italic(): void;
  strike(): void;
  highlight(): void;
  code(): void;
  blockquote(): void;
  externalLink(): void;
  bulletList(): void;
  orderedList(): void;
  taskList(): void;
  indent(): void;
  outdent(): void;
  insertTable(): void;
  codeBlock(): void;
  horizontalRule(): void;
  /** 0 removes the heading. */
  setHeading(level: number): void;
  /** Toggle an inline marker (`$`, `%%`…) around the selection. */
  wrap(marker: string): void;
  /** Replace the selection with `text`, caret `cursorOffset` chars into it. */
  insertText(text: string, cursorOffset: number): void;
  /** Run a raw command on the Tiptap editor. */
  exec(cmd: ViewCommand): void;
  addMarkdownRow?(): void;
  addMarkdownColumn?(): void;
  deleteMarkdownRow?(): void;
  deleteMarkdownColumn?(): void;
  deleteMarkdownTable?(): void;
  insertAttachment?(path: string, mime: string): void;
}

export interface ToolbarAction {
  id: string;
  title: string;
  icon?: string;
  run: () => void;
  active?: boolean;
}

export interface ToolbarCommand {
  id: string;
  title: string;
  icon: string;
  /** `configure` has no engine action: the store opens the config panel. */
  run?: (e: FormattingEngine) => void;
}

const clearFormatting: ViewCommand = (ed) => ed.chain().focus().unsetAllMarks().clearNodes().run();

const footnote: ViewCommand = (ed) => {
  const text = ed.getText();
  let n = 1;
  while (text.includes(`[^${n}]`)) n++;
  const ref = `[^${n}]`;
  ed.chain().focus().insertContent(ref).run();
  return ed
    .chain()
    .insertContentAt(ed.state.doc.content.size, {
      type: "paragraph",
      content: [{ type: "text", text: `${ref}: ` }],
    })
    .focus("end")
    .run();
};

const selectedText = (ed: Editor) => {
  const { from, to } = ed.state.selection;
  return ed.state.doc.textBetween(from, to, "\n");
};

const copy: ViewCommand = (ed) =>
  void navigator.clipboard?.writeText(selectedText(ed)).catch(() => {});
// Delete only once the text is safely on the clipboard.
const cut: ViewCommand = (ed) => {
  const text = selectedText(ed);
  if (text)
    void navigator.clipboard
      ?.writeText(text)
      .then(() => ed.chain().focus().deleteSelection().run())
      .catch(() => {});
};
const paste: ViewCommand = (ed) =>
  void navigator.clipboard
    ?.readText()
    .then((t) => ed.chain().focus().insertContent(t).run())
    .catch(() => {});

const toggleKeyboard: ViewCommand = (ed) =>
  ed.isFocused ? ed.commands.blur() : ed.commands.focus();

// The browser moves the caret by visual line/char; ProseMirror follows the DOM selection.
const moveCaret =
  (dir: "forward" | "backward", unit: "line" | "character"): ViewCommand =>
  (ed) => {
    ed.commands.focus();
    window.getSelection()?.modify("move", dir, unit);
  };

/** Swap the top-level block under the caret with its neighbour. */
const moveBlock =
  (delta: -1 | 1): ViewCommand =>
  (ed) => {
    const { $from } = ed.state.selection;
    const doc = ed.state.doc;
    const i = $from.index(0);
    const j = i + delta;
    if (j < 0 || j >= doc.childCount) return false;
    const [a, b] = delta < 0 ? [j, i] : [i, j];
    const start = $from.posAtIndex(a, 0);
    const first = doc.child(a);
    const second = doc.child(b);
    const end = start + first.nodeSize + second.nodeSize;
    // The caret keeps its offset inside the moved block.
    const offset = $from.pos - $from.posAtIndex(i, 0);
    const tr = ed.state.tr.replaceWith(start, end, [second, first]);
    const blockStart = delta < 0 ? start : start + second.nodeSize;
    ed.view.dispatch(tr.setSelection(TextSelection.near(tr.doc.resolve(blockStart + offset))));
    ed.commands.focus();
  };

const deleteBlock: ViewCommand = (ed) =>
  ed.chain().focus().selectParentNode().deleteSelection().run();

/** A formula from the selected text, or asked for when nothing is selected. */
const insertMath =
  (block: boolean): ViewCommand =>
  (ed) => {
    const { from, to } = ed.state.selection;
    const latex =
      ed.state.doc.textBetween(from, to, " ").trim() || window.prompt("Fórmula LaTeX", "");
    if (!latex?.trim()) return false;
    const c = ed.chain().focus().deleteSelection();
    return (block ? c.insertBlockMath({ latex }) : c.insertInlineMath({ latex })).run();
  };

const now = () => new Date();
const insertNow = (e: FormattingEngine, s: string) => e.insertText(s, s.length);

/** Every command the toolbar can show, in "available commands" order. */
export const TOOLBAR_COMMANDS: ToolbarCommand[] = [
  { id: "undo", title: "Deshacer", icon: "undo", run: (e) => e.undo() },
  { id: "redo", title: "Rehacer", icon: "redo", run: (e) => e.redo() },
  { id: "note-link", title: "Enlace a nota", icon: "note-link", run: (e) => e.noteLink() },
  { id: "embed", title: "Incrustar nota", icon: "embed", run: (e) => e.embed() },
  { id: "tag", title: "Etiqueta", icon: "tag", run: (e) => e.tag() },
  { id: "attachment", title: "Adjunto", icon: "attachment", run: (e) => e.attachment() },
  { id: "heading", title: "Título", icon: "heading", run: (e) => e.headingCycle() },
  { id: "bold", title: "Negrita", icon: "bold", run: (e) => e.bold() },
  { id: "italic", title: "Cursiva", icon: "italic", run: (e) => e.italic() },
  { id: "strike", title: "Tachado", icon: "strike", run: (e) => e.strike() },
  { id: "highlight", title: "Resaltar", icon: "highlight", run: (e) => e.highlight() },
  { id: "code", title: "Código en línea", icon: "code", run: (e) => e.code() },
  { id: "quote", title: "Cita", icon: "quote", run: (e) => e.blockquote() },
  { id: "comment", title: "Comentario", icon: "percent", run: (e) => e.wrap("%%") },
  { id: "link", title: "Enlace externo", icon: "link", run: (e) => e.externalLink() },
  { id: "bullet-list", title: "Lista", icon: "bullet-list", run: (e) => e.bulletList() },
  {
    id: "ordered-list",
    title: "Lista numerada",
    icon: "ordered-list",
    run: (e) => e.orderedList(),
  },
  { id: "checklist", title: "Tarea", icon: "checklist", run: (e) => e.taskList() },
  { id: "indent", title: "Aumentar sangría", icon: "indent", run: (e) => e.indent() },
  { id: "outdent", title: "Reducir sangría", icon: "outdent", run: (e) => e.outdent() },
  { id: "configure", title: "Configurar barra de herramientas", icon: "wrench" },
  { id: "remove-heading", title: "Quitar título", icon: "heading", run: (e) => e.setHeading(0) },
  ...[1, 2, 3, 4, 5, 6].map((n) => ({
    id: `h${n}`,
    title: `Título ${n}`,
    icon: "heading",
    run: (e: FormattingEngine) => e.setHeading(n),
  })),
  {
    id: "math",
    title: "Matemáticas en línea",
    icon: "sigma",
    run: (e) => e.exec(insertMath(false)),
  },
  {
    id: "clear-format",
    title: "Limpiar formato",
    icon: "eraser",
    run: (e) => e.exec(clearFormatting),
  },
  {
    id: "callout",
    title: "Insertar callout",
    icon: "quote",
    run: (e) =>
      e.exec((ed) => ed.chain().focus().toggleBlockquote().insertContent("[!note] ").run()),
  },
  { id: "codeblock", title: "Bloque de código", icon: "code", run: (e) => e.codeBlock() },
  { id: "hr", title: "Línea horizontal", icon: "minus", run: (e) => e.horizontalRule() },
  {
    id: "math-block",
    title: "Bloque de matemáticas",
    icon: "sigma",
    run: (e) => e.exec(insertMath(true)),
  },
  { id: "table", title: "Tabla 3×3", icon: "table", run: (e) => e.insertTable() },
  { id: "footnote", title: "Nota al pie", icon: "superscript", run: (e) => e.exec(footnote) },
  {
    id: "line-up",
    title: "Mover línea arriba",
    icon: "arrow-up",
    run: (e) => e.exec(moveBlock(-1)),
  },
  {
    id: "line-down",
    title: "Mover línea abajo",
    icon: "arrow-down",
    run: (e) => e.exec(moveBlock(1)),
  },
  {
    id: "caret-up",
    title: "Cursor arriba",
    icon: "chevron-up",
    run: (e) => e.exec(moveCaret("backward", "line")),
  },
  {
    id: "caret-down",
    title: "Cursor abajo",
    icon: "chevron-down",
    run: (e) => e.exec(moveCaret("forward", "line")),
  },
  {
    id: "caret-left",
    title: "Cursor a la izquierda",
    icon: "chevron-left",
    run: (e) => e.exec(moveCaret("backward", "character")),
  },
  {
    id: "caret-right",
    title: "Cursor a la derecha",
    icon: "chevron-right",
    run: (e) => e.exec(moveCaret("forward", "character")),
  },
  {
    id: "doc-start",
    title: "Ir a la primera línea",
    icon: "chevrons-up",
    run: (e) => e.exec((ed) => ed.commands.focus("start")),
  },
  {
    id: "doc-end",
    title: "Ir a la última línea",
    icon: "chevrons-down",
    run: (e) => e.exec((ed) => ed.commands.focus("end")),
  },
  {
    id: "keyboard",
    title: "Mostrar/ocultar teclado",
    icon: "keyboard",
    run: (e) => e.exec(toggleKeyboard),
  },
  { id: "delete-line", title: "Eliminar párrafo", icon: "trash", run: (e) => e.exec(deleteBlock) },
  {
    id: "table-add-row",
    title: "Tabla: añadir fila",
    icon: "table",
    run: (e) => e.addMarkdownRow?.(),
  },
  {
    id: "table-add-col",
    title: "Tabla: añadir columna",
    icon: "table",
    run: (e) => e.addMarkdownColumn?.(),
  },
  {
    id: "table-del-row",
    title: "Tabla: eliminar fila",
    icon: "table",
    run: (e) => e.deleteMarkdownRow?.(),
  },
  {
    id: "table-del-col",
    title: "Tabla: eliminar columna",
    icon: "table",
    run: (e) => e.deleteMarkdownColumn?.(),
  },
  {
    id: "table-delete",
    title: "Tabla: eliminar tabla",
    icon: "table",
    run: (e) => e.deleteMarkdownTable?.(),
  },
  { id: "cut", title: "Cortar", icon: "scissors", run: (e) => e.exec(cut) },
  { id: "copy", title: "Copiar", icon: "copy", run: (e) => e.exec(copy) },
  { id: "paste", title: "Pegar", icon: "clipboard", run: (e) => e.exec(paste) },
  {
    id: "date",
    title: "Insertar fecha actual",
    icon: "calendar",
    run: (e) => insertNow(e, now().toLocaleDateString("sv-SE")),
  },
  {
    id: "time",
    title: "Insertar hora actual",
    icon: "clock",
    run: (e) => insertNow(e, now().toTimeString().slice(0, 5)),
  },
];

export const COMMANDS_BY_ID = new Map(TOOLBAR_COMMANDS.map((c) => [c.id, c]));

/** Obsidian-mobile default: everything up to and including `configure`. */
export const DEFAULT_TOOLBAR = TOOLBAR_COMMANDS.slice(
  0,
  TOOLBAR_COMMANDS.findIndex((c) => c.id === "configure") + 1,
).map((c) => c.id);
