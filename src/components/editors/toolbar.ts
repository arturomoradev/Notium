// Formatting-toolbar contract for the Markdown editor (CodeMirror 6).
import type { EditorView } from "@codemirror/view";
import {
  cursorCharLeft,
  cursorCharRight,
  cursorDocEnd,
  cursorDocStart,
  cursorLineDown,
  cursorLineUp,
  deleteLine,
  moveLineDown,
  moveLineUp,
} from "@codemirror/commands";

export type ViewCommand = (view: EditorView) => boolean | void;

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
  /** Run a raw CodeMirror command on the view. */
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

const clearFormatting: ViewCommand = (v) => {
  const sel = v.state.selection.main;
  const line = v.state.doc.lineAt(sel.head);
  const [from, to] = sel.empty ? [line.from, line.to] : [sel.from, sel.to];
  const text = v.state.sliceDoc(from, to).replace(/\*\*|~~|==|%%|\*|`|\$/g, "");
  v.dispatch({ changes: { from, to, insert: text }, selection: { anchor: from + text.length } });
};

const footnote: ViewCommand = (v) => {
  const doc = v.state.doc.toString();
  let n = 1;
  while (doc.includes(`[^${n}]`)) n++;
  const ref = `[^${n}]`;
  const def = `${doc.endsWith("\n") ? "" : "\n"}\n${ref}: `;
  v.dispatch({
    changes: [
      { from: v.state.selection.main.to, insert: ref },
      { from: doc.length, insert: def },
    ],
    selection: { anchor: doc.length + ref.length + def.length },
    scrollIntoView: true,
  });
};

const selectedText = (v: EditorView) =>
  v.state.sliceDoc(v.state.selection.main.from, v.state.selection.main.to);

const copy: ViewCommand = (v) =>
  void navigator.clipboard?.writeText(selectedText(v)).catch(() => {});
// Delete only once the text is safely on the clipboard.
const cut: ViewCommand = (v) => {
  const text = selectedText(v);
  if (text)
    void navigator.clipboard
      ?.writeText(text)
      .then(() => v.dispatch(v.state.replaceSelection("")))
      .catch(() => {});
};
const paste: ViewCommand = (v) =>
  void navigator.clipboard
    ?.readText()
    .then((t) => v.dispatch(v.state.replaceSelection(t), { scrollIntoView: true }))
    .catch(() => {});

const toggleKeyboard: ViewCommand = (v) => (v.hasFocus ? v.contentDOM.blur() : v.focus());

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
  { id: "math", title: "Matemáticas en línea", icon: "sigma", run: (e) => e.wrap("$") },
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
    run: (e) => e.insertText("> [!note]\n> ", 12),
  },
  { id: "codeblock", title: "Bloque de código", icon: "code", run: (e) => e.codeBlock() },
  { id: "hr", title: "Línea horizontal", icon: "minus", run: (e) => e.horizontalRule() },
  {
    id: "math-block",
    title: "Bloque de matemáticas",
    icon: "sigma",
    run: (e) => e.insertText("$$\n\n$$", 3),
  },
  { id: "table", title: "Tabla 3×3", icon: "table", run: (e) => e.insertTable() },
  { id: "footnote", title: "Nota al pie", icon: "superscript", run: (e) => e.exec(footnote) },
  { id: "line-up", title: "Mover línea arriba", icon: "arrow-up", run: (e) => e.exec(moveLineUp) },
  {
    id: "line-down",
    title: "Mover línea abajo",
    icon: "arrow-down",
    run: (e) => e.exec(moveLineDown),
  },
  { id: "caret-up", title: "Cursor arriba", icon: "chevron-up", run: (e) => e.exec(cursorLineUp) },
  {
    id: "caret-down",
    title: "Cursor abajo",
    icon: "chevron-down",
    run: (e) => e.exec(cursorLineDown),
  },
  {
    id: "caret-left",
    title: "Cursor a la izquierda",
    icon: "chevron-left",
    run: (e) => e.exec(cursorCharLeft),
  },
  {
    id: "caret-right",
    title: "Cursor a la derecha",
    icon: "chevron-right",
    run: (e) => e.exec(cursorCharRight),
  },
  {
    id: "doc-start",
    title: "Ir a la primera línea",
    icon: "chevrons-up",
    run: (e) => e.exec(cursorDocStart),
  },
  {
    id: "doc-end",
    title: "Ir a la última línea",
    icon: "chevrons-down",
    run: (e) => e.exec(cursorDocEnd),
  },
  {
    id: "keyboard",
    title: "Mostrar/ocultar teclado",
    icon: "keyboard",
    run: (e) => e.exec(toggleKeyboard),
  },
  { id: "delete-line", title: "Eliminar párrafo", icon: "trash", run: (e) => e.exec(deleteLine) },
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
