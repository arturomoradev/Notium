import { EditorState, StateField } from "@codemirror/state";
import { Decoration, type DecorationSet, EditorView } from "@codemirror/view";
import type { MarkdownViewMode } from "../markdownEngine";
import { editBlockEffect, editingBlockField, forceDecorationsEffect } from "./caches";

const PERSISTENT_LANGS = new Set(["mermaid", "math", "latex", "tex"]);
import { isolateMarkdownFrontmatter } from "../markdownFormatting";
import { BulletWidget, CheckboxWidget, NumberWidget } from "./listWidgets";
import {
  AttachmentWidget,
  ExternalImageWidget,
  MarkdownLinkWidget,
  NoteEmbedWidget,
  WikiLinkWidget,
} from "./linkAndEmbedWidgets";
import {
  CalloutWidget,
  CodeBlockWidget,
  MarkdownTableWidget,
  MathWidget,
  MermaidDiagramWidget,
} from "./blockWidgets";

export function addInlineMark(
  lineStart: number,
  text: string,
  re: RegExp,
  markerLen: number,
  cls: string,
  decos: { from: number; to: number; value: Decoration }[],
  isOccupied: (from: number, to: number) => boolean,
  occupy: (from: number, to: number) => void,
): void {
  re.lastIndex = 0;
  let m: RegExpExecArray | null;
  while ((m = re.exec(text))) {
    const inner = m[1];
    if (!inner) continue;
    const localFrom = m.index;
    const localTo = m.index + m[0].length;
    if (isOccupied(localFrom, localTo)) continue;
    occupy(localFrom, localTo);
    const innerStart = m.index + m[0].indexOf(inner);
    const innerEnd = innerStart + inner.length;
    decos.push(
      Decoration.replace({}).range(lineStart + innerStart - markerLen, lineStart + innerStart),
    );
    decos.push(Decoration.mark({ class: cls }).range(lineStart + innerStart, lineStart + innerEnd));
    decos.push(
      Decoration.replace({}).range(lineStart + innerEnd, lineStart + innerEnd + markerLen),
    );
  }
}

export function buildLiveDecorations(
  state: EditorState,
  mode: MarkdownViewMode,
  onLinkActivate: (target: string) => void,
): DecorationSet {
  const decos: { from: number; to: number; value: Decoration }[] = [];
  const doc = state.doc;

  if (mode === "source") {
    for (let i = 1; i <= doc.lines; i++) {
      const line = doc.line(i);
      const wikiRe = /(?<!!)\[\[([^\[\]\n]+)\]\]/g;
      let m: RegExpExecArray | null;
      while ((m = wikiRe.exec(line.text))) {
        decos.push(
          Decoration.mark({ class: "cm-wiki-link" }).range(
            line.from + m.index,
            line.from + m.index + m[0].length,
          ),
        );
      }
    }
    return Decoration.set(decos, true);
  }

  const head = state.selection.main.head;
  const cursorLine = state.doc.lineAt(head).number;
  const revealLine = mode === "reading" ? -1 : cursorLine;
  // Diagrams, display formulas and tables stay rendered; only their edit button shows the source.
  const editable = mode !== "reading";
  const editing = state.field(editingBlockField, false) ?? null;

  // YAML frontmatter stays raw (its `---` fences are not horizontal rules).
  let firstLine = 1;
  if (doc.line(1).text === "---") {
    const { envelope } = isolateMarkdownFrontmatter(doc.toString());
    if (envelope) firstLine = doc.lineAt(envelope.length).number;
  }

  for (let i = firstLine; i <= doc.lines; i++) {
    const line = doc.line(i);
    const text = line.text;
    const start = line.from;
    const reveal = i === revealLine;
    // Cursor column on the active line (-1 elsewhere): only the element under it shows raw.
    const col = reveal ? head - start : -1;

    const fenceOpen = /^(\s*)```\s*([\w+#.-]*)\s*$/.exec(text);
    if (fenceOpen) {
      let last = i + 1;
      while (last <= doc.lines && !/^\s*```\s*$/.test(doc.line(last).text)) last++;
      if (last <= doc.lines) {
        const lang = (fenceOpen[2] ?? "").toLowerCase();
        const persistent = PERSISTENT_LANGS.has(lang);
        const cursorInside =
          mode !== "reading" &&
          (persistent ? editing?.from === start : cursorLine >= i && cursorLine <= last);
        if (cursorInside) {
          for (let row = i; row <= last; row++) {
            decos.push(Decoration.line({ class: "cm-code-line" }).range(doc.line(row).from));
          }
        } else {
          const bodyLines: string[] = [];
          for (let row = i + 1; row < last; row++) bodyLines.push(doc.line(row).text);
          const body = bodyLines.join("\n");
          const blockTo = doc.line(last).to;
          if (lang === "math" || lang === "latex" || lang === "tex") {
            decos.push(
              Decoration.replace({
                widget: new MathWidget(start, body, true, blockTo, editable),
                block: true,
              }).range(start, blockTo),
            );
          } else if (lang === "mermaid") {
            decos.push(
              Decoration.replace({
                widget: new MermaidDiagramWidget(start, body, blockTo, editable),
                block: true,
              }).range(start, blockTo),
            );
          } else {
            decos.push(
              Decoration.replace({
                widget: new CodeBlockWidget(start, lang, body),
                block: true,
              }).range(start, blockTo),
            );
          }
        }
        i = last;
        continue;
      }
      // Unclosed fence: the rest of the note is code, so leave it raw.
      break;
    }

    if (/^\s*\$\$\s*$/.test(text)) {
      let last = i + 1;
      while (last <= doc.lines && !/^\s*\$\$\s*$/.test(doc.line(last).text)) last++;
      if (last <= doc.lines) {
        if (!(editable && editing?.from === start)) {
          const bodyLines: string[] = [];
          for (let row = i + 1; row < last; row++) bodyLines.push(doc.line(row).text);
          decos.push(
            Decoration.replace({
              widget: new MathWidget(start, bodyLines.join("\n"), true, doc.line(last).to, editable),
              block: true,
            }).range(start, doc.line(last).to),
          );
        }
        i = last;
        continue;
      }
    }

    const singleLineDisplayMath = /^\s*\$\$([^$\n]+)\$\$\s*$/.exec(text);
    if (singleLineDisplayMath && !(editable && editing?.from === start)) {
      decos.push(
        Decoration.replace({
          widget: new MathWidget(start, singleLineDisplayMath[1], true, line.to, editable),
          block: true,
        }).range(start, line.to),
      );
      continue;
    }

    const calloutMatch = /^\s*>\s*\[!([A-Za-z0-9_-]+)\]([+-]?)\s*(.*)$/.exec(text);
    if (calloutMatch) {
      let last = i;
      while (last < doc.lines && /^\s*>/.test(doc.line(last + 1).text)) last++;
      const cursorInside = mode !== "reading" && cursorLine >= i && cursorLine <= last;
      if (!cursorInside) {
        const bodyLines: string[] = [];
        for (let row = i + 1; row <= last; row++) {
          bodyLines.push(doc.line(row).text.replace(/^\s*>\s?/, ""));
        }
        const foldSign = (
          calloutMatch[2] === "+" || calloutMatch[2] === "-" ? calloutMatch[2] : ""
        ) as "" | "+" | "-";
        decos.push(
          Decoration.replace({
            widget: new CalloutWidget(
              start,
              calloutMatch[1],
              foldSign,
              calloutMatch[3] ?? "",
              bodyLines,
            ),
            block: true,
          }).range(start, doc.line(last).to),
        );
        i = last;
        continue;
      }
    }

    const isTableStart =
      text.trimStart().startsWith("|") &&
      (i === 1 ||
        !doc
          .line(i - 1)
          .text.trimStart()
          .startsWith("|"));
    if (isTableStart) {
      let last = i;
      while (
        last < doc.lines &&
        doc
          .line(last + 1)
          .text.trimStart()
          .startsWith("|")
      )
        last++;
      if (last > i && !(editable && editing?.from === start)) {
        const rows: string[] = [];
        for (let row = i; row <= last; row++) rows.push(doc.line(row).text);
        decos.push(
          Decoration.replace({
            widget: new MarkdownTableWidget(start, doc.line(last).to, rows, editable),
            block: true,
          }).range(start, doc.line(last).to),
        );
        i = last;
        continue;
      }
    }

    if (/^\s*(?:-{3,}|\*{3,}|_{3,})\s*$/.test(text)) {
      decos.push(Decoration.line({ class: "cm-hr" }).range(start));
      if (!reveal) decos.push(Decoration.replace({}).range(start, line.to));
      continue;
    }

    const occupied: [number, number][] = [];
    const isOccupied = (from: number, to: number): boolean =>
      occupied.some(([a, b]) => from < b && to > a);
    const occupy = (from: number, to: number): void => {
      occupied.push([from, to]);
    };

    let m = /^(#{1,6})\s+/.exec(text);
    const isHeading = !!m;
    // List/quote markers show raw only while the cursor sits inside them.
    const hidePrefix = (len: number): boolean => col < 0 || col >= len;
    if (m) {
      const level = m[1].length;
      decos.push(Decoration.line({ class: `cm-h cm-h${level}` }).range(start));
      if (!reveal) {
        occupy(0, m[0].length);
        decos.push(Decoration.replace({}).range(start, start + m[0].length));
      }
    } else if ((m = /^(\s*)>\s?/.exec(text))) {
      decos.push(Decoration.line({ class: "cm-blockquote" }).range(start));
      occupy(0, m[0].length);
      if (hidePrefix(m[0].length)) {
        decos.push(Decoration.replace({}).range(start + m[1].length, start + m[0].length));
      }
    } else if ((m = /^(\s*)- \[([ xX])\]\s+/.exec(text))) {
      occupy(0, m[0].length);
      if (m[2] !== " " && start + m[0].length < line.to) {
        decos.push(Decoration.mark({ class: "cm-strike" }).range(start + m[0].length, line.to));
      }
      if (hidePrefix(m[0].length)) {
        const checkPos = start + m[1].length + 3;
        decos.push(
          Decoration.replace({
            widget: new CheckboxWidget(checkPos, m[2].toLowerCase() === "x"),
          }).range(start, start + m[0].length),
        );
      }
    } else if ((m = /^(\s*)[-*+]\s+/.exec(text))) {
      occupy(0, m[0].length);
      if (hidePrefix(m[0].length)) {
        decos.push(
          Decoration.replace({ widget: new BulletWidget() }).range(
            start + m[1].length,
            start + m[0].length,
          ),
        );
      }
    } else if ((m = /^(\s*)\d+[.)]\s+/.exec(text))) {
      occupy(0, m[0].length);
      if (hidePrefix(m[0].length)) {
        const marker = /\d+[.)]/.exec(m[0])?.[0] ?? "1.";
        decos.push(
          Decoration.replace({ widget: new NumberWidget(marker) }).range(
            start + m[1].length,
            start + m[0].length,
          ),
        );
      }
    }

    // Headings keep whole-line reveal.
    if (reveal && isHeading) {
      const codeRe = /`([^`\n]+?)`/g;
      let cm: RegExpExecArray | null;
      while ((cm = codeRe.exec(text))) {
        occupy(cm.index, cm.index + cm[0].length);
      }
      const wikiRe = /(?<!!)\[\[([^\[\]\n]+)\]\]/g;
      let wm: RegExpExecArray | null;
      while ((wm = wikiRe.exec(text))) {
        const localFrom = wm.index;
        const localTo = wm.index + wm[0].length;
        if (isOccupied(localFrom, localTo)) continue;
        occupy(localFrom, localTo);
        decos.push(
          Decoration.mark({ class: "cm-wiki-link" }).range(start + localFrom, start + localTo),
        );
      }
      continue;
    }

    const inlineFrom = occupied.length;
    const lineDecosFrom = decos.length;

    // 1. Inline code first so backtick-wrapped examples (`[[#Sección]]`, `$x$`, etc.) are protected
    addInlineMark(start, text, /`([^`\n]+?)`/g, 1, "cm-inline-code", decos, isOccupied, occupy);

    // 2. Wiki embeds `![[...]]`
    const embedRe = /!\[\[([^\[\]\n]+)\]\]/g;
    let embedMatch: RegExpExecArray | null;
    while ((embedMatch = embedRe.exec(text))) {
      const localFrom = embedMatch.index;
      const localTo = embedMatch.index + embedMatch[0].length;
      if (isOccupied(localFrom, localTo)) continue;
      const inner = embedMatch[1].trim();
      if (!inner) continue;
      occupy(localFrom, localTo);
      const isAtt =
        /^attachments\//i.test(inner) ||
        /\.(png|jpe?g|gif|webp|svg|pdf|mp3|mp4|wav|zip)$/i.test(inner);
      const widget = isAtt
        ? new AttachmentWidget(inner)
        : new NoteEmbedWidget(inner, onLinkActivate);
      decos.push(Decoration.replace({ widget }).range(start + localFrom, start + localTo));
    }

    // 3. Wiki links `[[...]]`
    const wikiRe = /(?<!!)\[\[([^\[\]\n]+)\]\]/g;
    let wikiMatch: RegExpExecArray | null;
    while ((wikiMatch = wikiRe.exec(text))) {
      const localFrom = wikiMatch.index;
      const localTo = wikiMatch.index + wikiMatch[0].length;
      if (isOccupied(localFrom, localTo)) continue;
      const inner = wikiMatch[1]?.trim();
      if (!inner) continue;
      occupy(localFrom, localTo);
      decos.push(
        Decoration.replace({
          widget: new WikiLinkWidget(inner, onLinkActivate),
        }).range(start + localFrom, start + localTo),
      );
    }

    // 4. External images `![alt](https://...)`
    const imageRe = /!\[([^\]]*)\]\((https?:\/\/[^)\s]+)\)/g;
    let imageMatch: RegExpExecArray | null;
    while ((imageMatch = imageRe.exec(text))) {
      const localFrom = imageMatch.index;
      const localTo = imageMatch.index + imageMatch[0].length;
      if (isOccupied(localFrom, localTo)) continue;
      occupy(localFrom, localTo);
      decos.push(
        Decoration.replace({
          widget: new ExternalImageWidget(imageMatch[1], imageMatch[2]),
        }).range(start + localFrom, start + localTo),
      );
    }

    // 5. External markdown links `[label](https://...)`
    const linkRe = /(?<!!)\[([^\]]+)\]\((https?:\/\/[^)\s]+)\)/g;
    let linkMatch: RegExpExecArray | null;
    while ((linkMatch = linkRe.exec(text))) {
      const localFrom = linkMatch.index;
      const localTo = linkMatch.index + linkMatch[0].length;
      if (isOccupied(localFrom, localTo)) continue;
      occupy(localFrom, localTo);
      decos.push(
        Decoration.replace({
          widget: new MarkdownLinkWidget(linkMatch[1], linkMatch[2]),
        }).range(start + localFrom, start + localTo),
      );
    }

    // 5b. Inline tags `#tag` (same boundary rules as the core tag scanner)
    const tagRe = /(?<=^|[\s({;,])#\p{L}[\p{L}\p{N}_\/-]*/gu;
    let tagMatch: RegExpExecArray | null;
    while ((tagMatch = tagRe.exec(text))) {
      const raw = tagMatch[0].replace(/[\/-]+$/, "");
      const localFrom = tagMatch.index;
      const localTo = localFrom + raw.length;
      if (raw.length < 2 || isOccupied(localFrom, localTo)) continue;
      occupy(localFrom, localTo);
      decos.push(Decoration.mark({ class: "cm-tag" }).range(start + localFrom, start + localTo));
    }

    // 6. Inline math `$...$`
    const mathRe = /(?<!\$)\$([^$\n]+?)\$(?!\$)/g;
    let mathMatch: RegExpExecArray | null;
    while ((mathMatch = mathRe.exec(text))) {
      const localFrom = mathMatch.index;
      const localTo = mathMatch.index + mathMatch[0].length;
      if (isOccupied(localFrom, localTo)) continue;
      const expr = mathMatch[1];
      if (!expr) continue;
      occupy(localFrom, localTo);
      decos.push(
        Decoration.replace({
          widget: new MathWidget(start + localFrom, expr, false),
        }).range(start + localFrom, start + localTo),
      );
    }

    // 7. Inline typography marks (`**bold**`, `*italic*`, `~~strike~~`, `==highlight==`)
    addInlineMark(start, text, /\*\*([^*\n]+?)\*\*/g, 2, "cm-bold", decos, isOccupied, occupy);
    addInlineMark(
      start,
      text,
      /(?<!\*)\*([^*\n]+?)\*(?!\*)/g,
      1,
      "cm-italic",
      decos,
      isOccupied,
      occupy,
    );
    addInlineMark(start, text, /~~([^~\n]+?)~~/g, 2, "cm-strike", decos, isOccupied, occupy);
    addInlineMark(start, text, /==([^=\n]+?)==/g, 2, "cm-highlight", decos, isOccupied, occupy);

    // Elements touching the cursor (edges included) stay raw, so `*a*` renders once you type past it.
    if (col >= 0) {
      for (const [a, b] of occupied.slice(inlineFrom)) {
        if (col < a || col > b) continue;
        for (let k = decos.length - 1; k >= lineDecosFrom; k--) {
          const d = decos[k];
          if (d.from < d.to && d.from >= start + a && d.to <= start + b) decos.splice(k, 1);
        }
      }
    }
  }

  decos.sort((a, b) => a.from - b.from || a.to - b.to);
  return Decoration.set(decos, true);
}

export function makeLivePreviewField(
  getMode: () => MarkdownViewMode,
  onLinkActivate: (target: string) => void,
) {
  const field = StateField.define<DecorationSet>({
    create(state) {
      return buildLiveDecorations(state, getMode(), onLinkActivate);
    },
    update(decos, tr) {
      const modeChanged =
        tr.effects.some((e) => e.is(forceDecorationsEffect) || e.is(editBlockEffect)) ||
        tr.startState.field(editingBlockField, false) !== tr.state.field(editingBlockField, false);
      if (!tr.docChanged && !tr.selection && !modeChanged) {
        return decos;
      }
      if (
        !tr.docChanged &&
        !modeChanged &&
        tr.startState.selection.main.head === tr.state.selection.main.head
      ) {
        return decos;
      }
      return buildLiveDecorations(tr.state, getMode(), onLinkActivate);
    },
    provide: (f) => EditorView.decorations.from(f),
  });
  return [editingBlockField, field];
}
