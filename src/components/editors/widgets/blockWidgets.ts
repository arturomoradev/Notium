import { EditorView, WidgetType } from "@codemirror/view";
import katex from "katex";
import "katex/dist/katex.min.css";
import { calloutCollapsedState } from "./caches";
import { codeLanguageLabel, renderHighlightedCode } from "./codeHighlight";
import { addEditButton, addViewerActions, openBlockSource } from "./blockViewer";

/**
 * Vertical gap around a block widget, as padding on a wrapper: CodeMirror ignores
 * margins when measuring block heights, so clicks and arrow keys would land on the wrong line.
 */
export function blockGap(el: HTMLElement): HTMLElement {
  const gap = document.createElement("div");
  gap.className = "cm-block-gap";
  gap.appendChild(el);
  return gap;
}

// Same marks and classes as the live preview; built with DOM nodes so cell text is never parsed as HTML.
const INLINE_MD =
  /`([^`]+?)`|\[\[([^\[\]]+)\]\]|\[([^\]]+)\]\((https?:\/\/[^)\s]+)\)|\*\*(.+?)\*\*|(?<!\*)\*([^*]+?)\*(?!\*)|~~(.+?)~~|==(.+?)==/g;

export function appendInlineMarkdown(parent: HTMLElement, text: string): void {
  let last = 0;
  for (const m of text.matchAll(INLINE_MD)) {
    parent.append(text.slice(last, m.index));
    last = m.index + m[0].length;
    const [, code, wiki, label, href, bold, italic, strike, mark] = m;
    if (code !== undefined) {
      const el = document.createElement("code");
      el.className = "cm-inline-code";
      el.textContent = code;
      parent.append(el);
    } else if (wiki !== undefined) {
      const el = document.createElement("span");
      el.className = "cm-wiki-link";
      el.textContent = wiki.split("|").pop()!.trim();
      parent.append(el);
    } else if (href !== undefined) {
      const el = document.createElement("a");
      el.className = "cm-rendered-link";
      el.href = href;
      el.target = "_blank";
      el.rel = "noopener noreferrer";
      appendInlineMarkdown(el, label);
      parent.append(el);
    } else {
      const el = document.createElement("span");
      el.className =
        bold !== undefined ? "cm-bold" : italic !== undefined ? "cm-italic" : strike !== undefined ? "cm-strike" : "cm-highlight";
      appendInlineMarkdown(el, bold ?? italic ?? strike ?? mark);
      parent.append(el);
    }
  }
  parent.append(text.slice(last));
}

/** `[from, to)` of each cell in a table row, without the outer pipes; `\|` is a literal pipe. */
export function tableCellRanges(line: string): [number, number][] {
  const cuts = [...line.matchAll(/(?<!\\)\|/g)].map((m) => m.index);
  const edges = [-1, ...cuts, line.length];
  const ranges: [number, number][] = [];
  for (let k = 0; k < edges.length - 1; k++) ranges.push([edges[k] + 1, edges[k + 1]]);
  if (cuts.length && /^\s*\|/.test(line)) ranges.shift();
  if (cuts.length && /(?<!\\)\|\s*$/.test(line) && ranges.length) ranges.pop();
  return ranges;
}

const cellText = (line: string, [a, b]: [number, number]) => line.slice(a, b).trim();

function renderCell(cell: HTMLElement, raw: string): void {
  cell.replaceChildren();
  appendInlineMarkdown(cell, raw.replace(/\\\|/g, "|"));
}

/**
 * Stays rendered while the cursor passes over it; cells are edited in place (raw markdown while
 * focused), and the edit button opens the whole table as markdown for structural changes.
 */
export class MarkdownTableWidget extends WidgetType {
  constructor(
    private readonly from: number,
    private readonly to: number,
    private readonly rows: string[],
    private readonly editable: boolean,
  ) {
    super();
  }
  eq(other: MarkdownTableWidget): boolean {
    return (
      other.editable === this.editable &&
      other.rows.join("\n") === this.rows.join("\n")
    );
  }
  private shape(): string {
    return `${this.editable}:` + this.rows.map((r) => tableCellRanges(r).length).join(",");
  }
  toDOM(view: EditorView): HTMLElement {
    const wrap = document.createElement("div");
    wrap.className = "cm-table-wrap";
    wrap.dataset.shape = this.shape();
    const table = document.createElement("table");
    table.className = "cm-rendered-table";
    this.rows.forEach((raw, rowIndex) => {
      if (/^\s*\|?\s*:?-{3,}/.test(raw)) return;
      const tr = document.createElement("tr");
      tableCellRanges(raw).forEach((range, col) => {
        const cell = document.createElement(rowIndex === 0 ? "th" : "td");
        cell.dataset.row = String(rowIndex);
        cell.dataset.col = String(col);
        renderCell(cell, cellText(raw, range));
        if (this.editable) makeCellEditable(view, wrap, cell);
        tr.appendChild(cell);
      });
      table.appendChild(tr);
    });
    wrap.appendChild(table);
    const block = blockGap(wrap);
    if (this.editable) {
      block.classList.add("cm-table-block");
      addEditButton(block, () => openBlockSource(view, wrap, this.to - this.from));
    }
    return block;
  }
  /** Same shape: refresh cells in place so the one being edited keeps its focus and caret. */
  updateDOM(dom: HTMLElement): boolean {
    const wrap = dom.querySelector<HTMLElement>(".cm-table-wrap");
    if (!wrap || wrap.dataset.shape !== this.shape()) return false;
    for (const cell of Array.from(wrap.querySelectorAll<HTMLElement>("th, td"))) {
      if (cell.contains(document.activeElement)) continue;
      const raw = this.rows[Number(cell.dataset.row)];
      renderCell(cell, cellText(raw, tableCellRanges(raw)[Number(cell.dataset.col)]));
    }
    return true;
  }
  ignoreEvent(event: Event): boolean {
    // Typing and clicks inside a cell belong to the cell, not to CodeMirror.
    return (event.target as Element | null)?.closest?.("th, td, .cm-block-action") != null;
  }
}

/** Reads the cell's line from the current document, so the handlers survive widget updates. */
function locateCell(view: EditorView, wrap: HTMLElement, cell: HTMLElement) {
  const tableLine = view.state.doc.lineAt(view.posAtDOM(wrap)).number;
  const lineNo = tableLine + Number(cell.dataset.row);
  if (lineNo > view.state.doc.lines) return null;
  const line = view.state.doc.line(lineNo);
  const range = tableCellRanges(line.text)[Number(cell.dataset.col)];
  return range ? { line, range } : null;
}

// A <textarea> rather than contenteditable: nested editing hosts never get focus inside CodeMirror.
function makeCellEditable(view: EditorView, wrap: HTMLElement, cell: HTMLElement): void {
  cell.classList.add("cm-cell-editable");
  cell.addEventListener("mousedown", (e) => {
    if (cell.querySelector("textarea")) return;
    e.preventDefault();
    editCell(view, wrap, cell);
  });
}

/** Edits one cell's raw markdown; Enter/blur saves, Escape cancels, Tab/Shift+Tab move between cells. */
let finishCellEdit: ((commit: boolean) => void) | null = null;

function editCell(view: EditorView, wrap: HTMLElement, cell: HTMLElement): void {
  // Save the cell being edited first; blur alone doesn't fire when the page lacks focus.
  finishCellEdit?.(true);
  const at = locateCell(view, wrap, cell);
  if (!at) return;
  const original = cellText(at.line.text, at.range);
  // Wraps like the rendered cell and grows with its text, so long cells stay fully visible.
  const input = document.createElement("textarea");
  input.className = "cm-cell-input";
  input.rows = 1;
  input.value = original;
  // Keep the column as wide as it was rendered: a textarea barely counts in table auto-layout.
  cell.style.width = `${cell.getBoundingClientRect().width}px`;
  cell.replaceChildren(input);
  const grow = () => {
    input.style.height = "auto";
    input.style.height = `${input.scrollHeight}px`;
  };
  grow();
  requestAnimationFrame(grow);
  input.addEventListener("input", grow);
  input.focus();
  input.setSelectionRange(original.length, original.length);
  let done = false;
  const finish = (commit: boolean) => {
    if (done) return;
    done = true;
    if (finishCellEdit === finish) finishCellEdit = null;
    // A table row is one line: pasted line breaks become spaces.
    const text = input.value.replace(/\s*\n\s*/g, " ").trim();
    cell.style.width = "";
    renderCell(cell, original); // removes the input; the update below re-renders the new text
    const now = locateCell(view, wrap, cell);
    if (commit && now && text !== original) {
      view.dispatch({
        changes: {
          from: now.line.from + now.range[0],
          to: now.line.from + now.range[1],
          insert: ` ${text.replace(/(?<!\\)\|/g, "\\|")} `,
        },
        userEvent: "input",
      });
    }
  };
  finishCellEdit = finish;
  input.addEventListener("blur", () => finish(true));
  input.addEventListener("keydown", (e) => {
    if (e.key === "Enter" || e.key === "Escape") {
      e.preventDefault();
      finish(e.key === "Enter");
      view.focus();
    } else if (e.key === "Tab") {
      e.preventDefault();
      const cells = Array.from(wrap.querySelectorAll<HTMLElement>("th, td"));
      const next = cells[cells.indexOf(cell) + (e.shiftKey ? -1 : 1)];
      finish(true);
      if (next) editCell(view, wrap, next);
      else view.focus();
    }
  });
}

/** KaTeX HTML for a formula; invalid TeX shows the source in red instead of throwing. */
export function renderLatexToHtml(src: string, displayMode = false): string {
  return katex.renderToString(src.trim(), { displayMode, throwOnError: false });
}

export class MathWidget extends WidgetType {
  constructor(
    private readonly from: number,
    private readonly expr: string,
    private readonly displayMode: boolean,
    private readonly to = from,
    private readonly editable = false,
  ) {
    super();
  }
  eq(other: MathWidget): boolean {
    return (
      other.expr === this.expr &&
      other.displayMode === this.displayMode &&
      other.editable === this.editable
    );
  }
  toDOM(view: EditorView): HTMLElement {
    if (!this.displayMode) {
      const el = document.createElement("span");
      el.className = "cm-math-inline";
      el.innerHTML = renderLatexToHtml(this.expr);
      el.title = this.expr;
      el.addEventListener("mousedown", (e) => {
        e.preventDefault();
        view.dispatch({ selection: { anchor: view.posAtDOM(el) } });
        view.focus();
      });
      return el;
    }
    const el = document.createElement("div");
    el.className = "cm-math-block";
    const content = document.createElement("span");
    content.className = "cm-math-content";
    content.innerHTML = renderLatexToHtml(this.expr, true);
    el.append(content);
    addViewerActions(el, {
      code: this.expr,
      name: "formula",
      image: () => content,
      onEdit: this.editable ? () => openBlockSource(view, el, this.to - this.from) : undefined,
    });
    return blockGap(el);
  }
  ignoreEvent(): boolean {
    return false;
  }
}

export const CALLOUT_META: Record<string, { icon: string; label: string; kind: string }> = {
  note: { icon: "📝", label: "Nota", kind: "note" },
  info: { icon: "ℹ️", label: "Info", kind: "info" },
  tip: { icon: "💡", label: "Consejo", kind: "tip" },
  hint: { icon: "💡", label: "Sugerencia", kind: "tip" },
  important: { icon: "📌", label: "Importante", kind: "important" },
  success: { icon: "✅", label: "Correcto", kind: "success" },
  check: { icon: "✅", label: "Verificado", kind: "success" },
  warning: { icon: "⚠️", label: "Advertencia", kind: "warning" },
  caution: { icon: "⚠️", label: "Precaución", kind: "warning" },
  danger: { icon: "🚨", label: "Peligro", kind: "danger" },
  error: { icon: "❌", label: "Error", kind: "danger" },
  bug: { icon: "🐛", label: "Bug", kind: "danger" },
  question: { icon: "❓", label: "Pregunta", kind: "info" },
  quote: { icon: "💬", label: "Cita", kind: "quote" },
};

export class CalloutWidget extends WidgetType {
  constructor(
    private readonly from: number,
    private readonly rawType: string,
    private readonly foldSign: "" | "+" | "-",
    private readonly titleText: string,
    private readonly bodyLines: string[],
  ) {
    super();
  }
  eq(other: CalloutWidget): boolean {
    return (
      other.from === this.from &&
      other.rawType === this.rawType &&
      other.foldSign === this.foldSign &&
      other.titleText === this.titleText &&
      other.bodyLines.join("\n") === this.bodyLines.join("\n")
    );
  }
  toDOM(view: EditorView): HTMLElement {
    const key = this.rawType.toLowerCase();
    const meta = CALLOUT_META[key] ?? { icon: "📝", label: this.rawType, kind: "note" };
    const isFoldable = this.foldSign === "+" || this.foldSign === "-";
    const stateKey = `${this.from}:${this.rawType}:${this.titleText}`;
    let collapsed = isFoldable
      ? (calloutCollapsedState.get(stateKey) ?? this.foldSign === "-")
      : false;

    const wrap = document.createElement("div");
    wrap.className = `cm-callout cm-callout-${meta.kind}` + (collapsed ? " is-collapsed" : "");

    const head = document.createElement("div");
    head.className = "cm-callout-title" + (isFoldable ? " is-foldable" : "");
    const iconSpan = document.createElement("span");
    iconSpan.className = "cm-callout-icon";
    iconSpan.textContent = meta.icon;
    const titleSpan = document.createElement("span");
    titleSpan.className = "cm-callout-title-text";
    titleSpan.textContent = this.titleText.trim() || meta.label;
    head.append(iconSpan, titleSpan);

    let foldChevron: HTMLSpanElement | undefined;
    if (isFoldable) {
      foldChevron = document.createElement("span");
      foldChevron.className = "cm-callout-fold";
      foldChevron.textContent = collapsed ? "▸" : "▾";
      foldChevron.title = collapsed ? "Desplegar callout" : "Plegar callout";
      head.appendChild(foldChevron);
    }

    const editBtn = document.createElement("button");
    editBtn.type = "button";
    editBtn.className = "cm-callout-edit";
    editBtn.textContent = "✎";
    editBtn.title = "Editar código del callout";
    editBtn.addEventListener("mousedown", (e) => {
      e.preventDefault();
      e.stopPropagation();
      view.dispatch({ selection: { anchor: this.from } });
      view.focus();
    });
    head.appendChild(editBtn);
    wrap.appendChild(head);

    let body: HTMLDivElement | undefined;
    if (this.bodyLines.length > 0) {
      body = document.createElement("div");
      body.className = "cm-callout-body";
      body.textContent = this.bodyLines.join("\n");
      if (collapsed) body.style.display = "none";
      wrap.appendChild(body);
    }

    if (isFoldable) {
      head.addEventListener("mousedown", (e) => {
        if ((e.target as Element | null)?.closest(".cm-callout-edit")) return;
        e.preventDefault();
        e.stopPropagation();
        collapsed = !collapsed;
        calloutCollapsedState.set(stateKey, collapsed);
        wrap.classList.toggle("is-collapsed", collapsed);
        if (foldChevron) {
          foldChevron.textContent = collapsed ? "▸" : "▾";
          foldChevron.title = collapsed ? "Desplegar callout" : "Plegar callout";
        }
        if (body) {
          body.style.display = collapsed ? "none" : "";
        }
        view.requestMeasure();
      });
      if (body) {
        body.addEventListener("mousedown", (e) => {
          e.preventDefault();
          view.dispatch({ selection: { anchor: this.from } });
          view.focus();
        });
      }
    } else {
      wrap.addEventListener("mousedown", (e) => {
        e.preventDefault();
        view.dispatch({ selection: { anchor: this.from } });
        view.focus();
      });
    }
    return blockGap(wrap);
  }
  ignoreEvent(): boolean {
    return false;
  }
}

type Mermaid = typeof import("mermaid").default;
let mermaidModule: Promise<Mermaid> | undefined;
let mermaidTheme = "";
let mermaidSeq = 0;
let mermaidQueue: Promise<unknown> = Promise.resolve();

/**
 * Render Mermaid source to SVG, one diagram at a time: overlapping `mermaid.render` calls share
 * its internal state and produce broken or empty diagrams.
 */
function renderMermaid(code: string): Promise<string> {
  const next = mermaidQueue.then(() => renderMermaidNow(code));
  mermaidQueue = next.catch(() => {});
  return next.then((svg) => {
    if (mermaidSvgCache.size > 100) mermaidSvgCache.clear();
    mermaidSvgCache.set(mermaidKey(code), svg);
    return svg;
  });
}

/** Finished SVGs, so a diagram rebuilt by the editor shows at once instead of flashing its code. */
const mermaidSvgCache = new Map<string, string>();
const mermaidKey = (code: string) => `${document.documentElement.dataset.theme ?? ""}\n${code}`;

/** Mermaid is loaded on first use, so notes without diagrams never pay for it. */
async function renderMermaidNow(code: string): Promise<string> {
  const mermaid = await (mermaidModule ??= import("mermaid").then((m) => m.default));
  const theme = document.documentElement.dataset.theme === "light" ? "default" : "dark";
  if (theme !== mermaidTheme) {
    mermaid.initialize({
      startOnLoad: false,
      securityLevel: "strict",
      theme,
      fontFamily: "inherit",
      // Errors are shown by the widget; don't let Mermaid append its error SVG to <body>.
      suppressErrorRendering: true,
    });
    mermaidTheme = theme;
  }
  const { svg } = await mermaid.render(`notium-mermaid-${++mermaidSeq}`, code);
  return svg;
}

export class MermaidDiagramWidget extends WidgetType {
  constructor(
    private readonly from: number,
    private readonly code: string,
    private readonly to: number,
    private readonly editable: boolean,
  ) {
    super();
  }
  eq(other: MermaidDiagramWidget): boolean {
    return other.code === this.code && other.editable === this.editable;
  }
  toDOM(view: EditorView): HTMLElement {
    const container = document.createElement("div");
    container.className = "cm-mermaid-widget";

    const badge = document.createElement("div");
    badge.className = "cm-block-badge";
    badge.textContent = "Diagrama Mermaid";
    container.appendChild(badge);

    const showDiagram = (svg: string) => {
      const diagram = document.createElement("div");
      diagram.className = "cm-mermaid-diagram";
      // Sanitized by Mermaid itself (securityLevel "strict" runs DOMPurify).
      diagram.innerHTML = svg;
      return diagram;
    };
    const cached = mermaidSvgCache.get(mermaidKey(this.code));
    if (cached !== undefined) {
      container.appendChild(showDiagram(cached));
    } else {
      // Raw code until Mermaid has loaded and rendered (and if the text is invalid).
      const holder = document.createElement("pre");
      holder.className = "cm-mermaid-fallback";
      holder.textContent = this.code;
      container.appendChild(holder);

      // No "destroyed" guard: CodeMirror can rebuild the DOM from the same widget instance, so a
      // per-instance flag set by the old DOM left the new one stuck on the raw code.
      void renderMermaid(this.code)
        .then((svg) => {
          holder.replaceWith(showDiagram(svg));
          view.requestMeasure();
        })
        .catch((e: unknown) => {
          const error = document.createElement("div");
          error.className = "cm-mermaid-error";
          error.textContent = e instanceof Error ? e.message : String(e);
          container.appendChild(error);
          view.requestMeasure();
        });
    }

    addViewerActions(container, {
      code: this.code,
      name: "diagrama",
      image: () => container.querySelector(".cm-mermaid-diagram svg"),
      onEdit: this.editable ? () => openBlockSource(view, container, this.to - this.from) : undefined,
    });
    return blockGap(container);
  }
  ignoreEvent(): boolean {
    return false;
  }
}

export class CodeBlockWidget extends WidgetType {
  constructor(
    private readonly from: number,
    private readonly lang: string,
    private readonly code: string,
  ) {
    super();
  }
  eq(other: CodeBlockWidget): boolean {
    return other.from === this.from && other.lang === this.lang && other.code === this.code;
  }
  toDOM(view: EditorView): HTMLElement {
    const wrap = document.createElement("div");
    wrap.className = "cm-codeblock-widget";
    if (this.lang) {
      const badge = document.createElement("div");
      badge.className = "cm-block-badge cm-code-lang";
      badge.textContent = codeLanguageLabel(this.lang);
      wrap.appendChild(badge);
    }
    const pre = document.createElement("pre");
    const codeEl = document.createElement("code");
    renderHighlightedCode(codeEl, this.lang, this.code);
    pre.appendChild(codeEl);
    wrap.appendChild(pre);
    wrap.addEventListener("mousedown", (e) => {
      e.preventDefault();
      view.dispatch({ selection: { anchor: this.from } });
      view.focus();
    });
    return blockGap(wrap);
  }
  ignoreEvent(): boolean {
    return false;
  }
}
