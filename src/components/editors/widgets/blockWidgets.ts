import { EditorView, WidgetType } from "@codemirror/view";
import { calloutCollapsedState } from "./caches";
import { codeLanguageLabel, renderHighlightedCode } from "./codeHighlight";

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

export class MarkdownTableWidget extends WidgetType {
  constructor(
    private readonly from: number,
    private readonly rows: string[],
  ) {
    super();
  }
  eq(other: MarkdownTableWidget): boolean {
    return other.from === this.from && other.rows.join("\n") === this.rows.join("\n");
  }
  toDOM(view: EditorView): HTMLElement {
    const wrap = document.createElement("div");
    wrap.className = "cm-table-wrap";
    const table = document.createElement("table");
    table.className = "cm-rendered-table";
    this.rows.forEach((raw, rowIndex) => {
      if (/^\s*\|?\s*:?-{3,}/.test(raw)) return;
      const tr = document.createElement("tr");
      const cells = raw.trim().replace(/^\|/, "").replace(/\|$/, "").split("|");
      cells.forEach((rawCell) => {
        const cell = document.createElement(rowIndex === 0 ? "th" : "td");
        appendInlineMarkdown(cell, rawCell.trim());
        tr.appendChild(cell);
      });
      table.appendChild(tr);
    });
    wrap.appendChild(table);
    wrap.addEventListener("mousedown", (event) => {
      // A press on the horizontal scrollbar must drag it, not open the raw markdown.
      if (event.target === wrap && event.offsetY > wrap.clientHeight) {
        event.stopPropagation();
        return;
      }
      event.preventDefault();
      view.dispatch({ selection: { anchor: this.from } });
      view.focus();
    });
    return blockGap(wrap);
  }
  ignoreEvent(): boolean {
    return false;
  }
}

export const TEX_SYMBOLS: Record<string, string> = {
  alpha: "α",
  beta: "β",
  gamma: "γ",
  delta: "δ",
  epsilon: "ε",
  zeta: "ζ",
  eta: "η",
  theta: "θ",
  iota: "ι",
  kappa: "κ",
  lambda: "λ",
  mu: "μ",
  nu: "ν",
  xi: "ξ",
  pi: "π",
  rho: "ρ",
  sigma: "σ",
  tau: "τ",
  upsilon: "υ",
  phi: "φ",
  chi: "χ",
  psi: "ψ",
  omega: "ω",
  Gamma: "Γ",
  Delta: "Δ",
  Theta: "Θ",
  Lambda: "Λ",
  Xi: "Ξ",
  Pi: "Π",
  Sigma: "Σ",
  Phi: "Φ",
  Psi: "Ψ",
  Omega: "Ω",
  int: "∫",
  sum: "∑",
  prod: "∏",
  infty: "∞",
  partial: "∂",
  nabla: "∇",
  pm: "±",
  mp: "∓",
  times: "×",
  div: "÷",
  cdot: "·",
  leq: "≤",
  le: "≤",
  geq: "≥",
  ge: "≥",
  neq: "≠",
  ne: "≠",
  approx: "≈",
  equiv: "≡",
  to: "→",
  rightarrow: "→",
  leftarrow: "←",
  Rightarrow: "⇒",
  Leftarrow: "⇐",
  forall: "∀",
  exists: "∃",
  in: "∈",
  notin: "∉",
  subset: "⊂",
  supset: "⊃",
  cup: "∪",
  cap: "∩",
};

export function renderLatexToHtml(src: string): string {
  const esc = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
  let out = esc(src.trim());
  out = out.replace(
    /\\frac\{([^{}]+)\}\{([^{}]+)\}/g,
    '<span class="cm-math-frac"><span class="cm-math-num">$1</span><span class="cm-math-den">$2</span></span>',
  );
  out = out.replace(/\\sqrt\{([^{}]+)\}/g, '√<span class="cm-math-sqrt">$1</span>');
  out = out.replace(/\\text\{([^{}]+)\}/g, '<span class="cm-math-text">$1</span>');
  out = out.replace(/\\([a-zA-Z]+)/g, (_m, cmd: string) => TEX_SYMBOLS[cmd] ?? cmd);
  out = out
    .replace(/\^\{([^{}]+)\}/g, "<sup>$1</sup>")
    .replace(/\^([a-zA-Z0-9+-])/g, "<sup>$1</sup>");
  out = out
    .replace(/_\{([^{}]+)\}/g, "<sub>$1</sub>")
    .replace(/_([a-zA-Z0-9+-])/g, "<sub>$1</sub>");
  return out;
}

export class MathWidget extends WidgetType {
  constructor(
    private readonly from: number,
    private readonly expr: string,
    private readonly displayMode: boolean,
  ) {
    super();
  }
  eq(other: MathWidget): boolean {
    return (
      other.from === this.from && other.expr === this.expr && other.displayMode === this.displayMode
    );
  }
  toDOM(view: EditorView): HTMLElement {
    const el = document.createElement(this.displayMode ? "div" : "span");
    el.className = this.displayMode ? "cm-math-block" : "cm-math-inline";
    el.innerHTML = renderLatexToHtml(this.expr);
    el.title = this.expr;
    el.addEventListener("mousedown", (e) => {
      e.preventDefault();
      view.dispatch({ selection: { anchor: this.from } });
      view.focus();
    });
    return this.displayMode ? blockGap(el) : el;
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

/** Render Mermaid source to SVG. Mermaid is loaded on first use, so notes without diagrams never pay for it. */
async function renderMermaid(code: string): Promise<string> {
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
  private destroyed = false;

  constructor(
    private readonly from: number,
    private readonly code: string,
  ) {
    super();
  }
  eq(other: MermaidDiagramWidget): boolean {
    return other.from === this.from && other.code === this.code;
  }
  toDOM(view: EditorView): HTMLElement {
    const container = document.createElement("div");
    container.className = "cm-mermaid-widget";

    const badge = document.createElement("div");
    badge.className = "cm-block-badge";
    badge.textContent = "Diagrama Mermaid";
    container.appendChild(badge);

    // Raw code until Mermaid has loaded and rendered (and if the text is invalid).
    const holder = document.createElement("pre");
    holder.className = "cm-mermaid-fallback";
    holder.textContent = this.code;
    container.appendChild(holder);

    void renderMermaid(this.code)
      .then((svg) => {
        if (this.destroyed) return;
        const diagram = document.createElement("div");
        diagram.className = "cm-mermaid-diagram";
        // Sanitized by Mermaid itself (securityLevel "strict" runs DOMPurify).
        diagram.innerHTML = svg;
        holder.replaceWith(diagram);
        view.requestMeasure();
      })
      .catch((e: unknown) => {
        if (this.destroyed) return;
        const error = document.createElement("div");
        error.className = "cm-mermaid-error";
        error.textContent = e instanceof Error ? e.message : String(e);
        container.appendChild(error);
        view.requestMeasure();
      });

    container.addEventListener("mousedown", (e) => {
      e.preventDefault();
      view.dispatch({ selection: { anchor: this.from } });
      view.focus();
    });
    return blockGap(container);
  }
  destroy(): void {
    this.destroyed = true;
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
