// CodeMirror 6 engine for `.md` notes with Obsidian-style Live Preview.
import { Compartment, EditorState, Prec, Transaction, type Extension } from "@codemirror/state";
import {
  EditorView,
  ViewPlugin,
  dropCursor,
  highlightSpecialChars,
  keymap,
} from "@codemirror/view";
import {
  defaultKeymap,
  history,
  historyKeymap,
  indentLess,
  indentMore,
  redo,
  undo,
} from "@codemirror/commands";
import { bracketMatching, indentOnInput, syntaxHighlighting } from "@codemirror/language";
import { markdown } from "@codemirror/lang-markdown";
import { languages } from "@codemirror/language-data";
import { noteHighlightStyle } from "./widgets/codeHighlight";
import { autocompletion, type CompletionSource } from "@codemirror/autocomplete";
import type { EditorEngine, EditorEngineCallbacks } from "./engine";
import type { FormattingEngine } from "./toolbar";
import { scheduleCaretReveal, updateViewport } from "../../lib/mobile/viewport";
import { mdToPlainText } from "../../lib/plainText";
import { call } from "../../lib/ipc/commands";
import {
  editMarkdownTable,
  insertMarkdown,
  toggleMarkdownPrefix,
  wrapMarkdownSelection,
  type MarkdownTableOperation,
} from "./markdownFormatting";
import {
  buildWikiCompletionEntries,
  forceDecorationsEffect,
  invalidateEmbedCache,
  normalizeHeading,
  type WikiCompletionEntry,
} from "./widgets/caches";
import { makeLivePreviewField } from "./widgets/liveDecorations";
import { inlineTitle } from "./widgets/inlineTitle";

export { invalidateEmbedCache };
export type MarkdownViewMode = "live" | "source" | "reading";

export interface MarkdownEngine extends EditorEngine, FormattingEngine {
  setMode(mode: MarkdownViewMode): void;
  getMode(): MarkdownViewMode;
  blur(): void;
  getPlainText(): string;
  insertAttachment(path: string, mime: string): void;
  scrollToHeading(heading: string): boolean;
}

// Android keyboards read the raw "- [ ] " before the caret (CodeMirror uses EditContext
// there) and see no sentence start, so the first letter of a task stays lowercase.
const capitalizeAfterTask = EditorState.transactionFilter.of((tr) => {
  if (!tr.docChanged || !tr.isUserEvent("input")) return tr;
  let fix: { from: number; to: number; insert: string } | undefined;
  tr.changes.iterChangedRanges((_fromA, _toA, fromB, toB) => {
    if (toB === fromB) return;
    const line = tr.newDoc.lineAt(fromB);
    const marker = /^\s*[-*+] \[[ xX]\] /.exec(line.text);
    if (!marker || fromB !== line.from + marker[0].length) return;
    const ch = line.text[marker[0].length];
    const upper = ch.toLocaleUpperCase();
    if (upper !== ch && upper.length === 1) fix = { from: fromB, to: fromB + 1, insert: upper };
  });
  return fix ? [tr, { changes: fix, sequential: true }] : tr;
});

export function mountMarkdown(
  host: HTMLElement,
  cb: EditorEngineCallbacks,
  initialMode: MarkdownViewMode = "live",
): MarkdownEngine {
  let applyingExternal = false;
  let mode: MarkdownViewMode = initialMode;
  let lastValidSelection = { anchor: 0, head: 0 };
  let customCompletionMenu: HTMLDivElement | undefined;
  let customCompletionItems: WikiCompletionEntry[] = [];
  let customCompletionIndex = 0;
  let customCompletionStart = 0;
  let customCompletionCursor = 0;
  let customCompletionPrefix = "[[";
  let customCompletionToken = 0;
  let destroyed = false;
  let tagCache: { ts: number; tags: string[] } | null = null;

  function selection(): { from: number; to: number } {
    const current = view.state.selection.main;
    const selected = view.hasFocus ? current : lastValidSelection;
    const len = view.state.doc.length;
    return {
      from: Math.min(selected.anchor, selected.head, len),
      to: Math.min(Math.max(selected.anchor, selected.head), len),
    };
  }

  const livePreviewField = makeLivePreviewField(
    () => mode,
    (t) => cb.onLinkActivate(t),
  );

  const linkClick = ViewPlugin.fromClass(class {}, {
    eventHandlers: {
      mousedown(event) {
        if (event.button !== 0) return false;
        const targetNode = event.target as Node | null;
        const targetEl =
          targetNode instanceof Element ? targetNode : (targetNode?.parentElement ?? null);
        const el = targetEl?.closest(".cm-wiki-link");
        if (!el || el.classList.contains("cm-wiki-link-pill")) return false;
        const raw = el.textContent ?? "";
        const m = /^\[\[(.+)\]\]$/.exec(raw.trim());
        if (!m) return false;
        event.preventDefault();
        cb.onLinkActivate(m[1].trim());
        return true;
      },
    },
  });

  const tagCompletion: CompletionSource = (ctx) => {
    const line = ctx.state.doc.lineAt(ctx.pos);
    const textBefore = line.text.slice(0, ctx.pos - line.from);
    if (
      /!?\[\[[^\]\n]*$/.test(textBefore) ||
      /\]\([^)\n]*$/.test(textBefore) ||
      /^\s*#+\s*$/.test(textBefore)
    ) {
      return null;
    }
    const before = ctx.matchBefore(/(?:^|\s)#[\w\u00C0-\u024F/-]*/);
    if (!before) return null;
    const hashOffset = before.text.indexOf("#");
    const from = before.from + hashOffset;
    const query = before.text.slice(hashOffset + 1);
    if (!query && !ctx.explicit) return null;
    // One note_list per 10 s instead of one per keystroke.
    const tags =
      tagCache && Date.now() - tagCache.ts < 10_000
        ? Promise.resolve(tagCache.tags)
        : call("note_list").then((notes) => {
            tagCache = { ts: Date.now(), tags: [...new Set(notes.flatMap((note) => note.tags))] };
            return tagCache.tags;
          });
    return tags.then((all) => ({
      from,
      options: all
        .filter((tag) => tag.toLowerCase().startsWith(query.toLowerCase()))
        .map((tag) => ({ label: `#${tag}`, type: "keyword", apply: `#${tag} ` })),
    }));
  };

  function closeCustomCompletion(): void {
    customCompletionToken++;
    customCompletionMenu?.remove();
    customCompletionMenu = undefined;
    customCompletionItems = [];
  }

  function applyCustomCompletion(item: WikiCompletionEntry): void {
    const closing =
      view.state.sliceDoc(customCompletionCursor, customCompletionCursor + 2) === "]]"
        ? customCompletionCursor + 2
        : customCompletionCursor;
    if (item.keepOpenForSection) {
      const insert = `${customCompletionPrefix}${item.insertValue}`;
      view.focus();
      view.dispatch({
        changes: { from: customCompletionStart, to: customCompletionCursor, insert },
        selection: { anchor: customCompletionStart + insert.length },
      });
      return;
    }
    const insert = `${customCompletionPrefix}${item.insertValue}]]`;
    closeCustomCompletion();
    view.focus();
    view.dispatch({
      changes: { from: customCompletionStart, to: closing, insert },
      selection: { anchor: customCompletionStart + insert.length },
    });
  }

  async function showCustomCompletion(
    query: string,
    from: number,
    cursor: number,
    prefix: string,
  ): Promise<void> {
    customCompletionStart = from;
    customCompletionCursor = cursor;
    customCompletionPrefix = prefix;
    customCompletionIndex = 0;
    const token = ++customCompletionToken;
    try {
      const items = await buildWikiCompletionEntries(query, view.state.doc.toString());
      if (destroyed || token !== customCompletionToken || mode !== "live") return;
      customCompletionItems = items;
      if (!customCompletionMenu) {
        customCompletionMenu = document.createElement("div");
        customCompletionMenu.className = "cm-tooltip cm-tooltip-autocomplete cm-wiki-completion";
        customCompletionMenu.setAttribute("role", "listbox");
        document.body.appendChild(customCompletionMenu);
      }
      customCompletionMenu.textContent = "";
      if (customCompletionItems.length === 0) {
        const empty = document.createElement("div");
        empty.className = "cm-completionEmpty";
        empty.textContent = query.includes("#")
          ? "No hay secciones coincidentes"
          : "No hay notas coincidentes";
        customCompletionMenu.appendChild(empty);
      }
      customCompletionItems.forEach((item, index) => {
        const row = document.createElement("button");
        row.type = "button";
        row.className = "cm-completionLabel" + (index === customCompletionIndex ? " selected" : "");
        row.textContent = item.label;
        row.addEventListener("mousedown", (event) => {
          event.preventDefault();
          applyCustomCompletion(item);
        });
        customCompletionMenu!.appendChild(row);
      });
      positionCustomCompletion();
    } catch {
      closeCustomCompletion();
    }
  }

  function positionCustomCompletion(): void {
    if (!customCompletionMenu) return;
    const editorRect = view.dom.getBoundingClientRect();
    let left = editorRect.left + 16;
    let top = editorRect.top + 48;
    try {
      const coords = view.coordsAtPos(customCompletionCursor);
      if (coords) {
        left = coords.left;
        top = coords.bottom + 4;
      }
    } catch {
      /* fall back to the editor's top edge */
    }
    customCompletionMenu.style.left = `${Math.max(8, Math.min(left, window.innerWidth - 280))}px`;
    customCompletionMenu.style.top = `${Math.max(8, Math.min(top, window.innerHeight - 240))}px`;
  }

  function refreshCustomCompletion(): void {
    if (destroyed || mode !== "live") return closeCustomCompletion();
    const { head } = view.state.selection.main;
    const line = view.state.doc.lineAt(head);
    const before = line.text.slice(0, head - line.from);
    const match = /(!?\[\[)([^\]\n]*)$/.exec(before);
    if (!match) {
      closeCustomCompletion();
      return;
    }
    void showCustomCompletion(match[2], line.from + match.index, head, match[1]);
  }

  function moveCustomCompletion(delta: number): boolean {
    if (!customCompletionMenu) return false;
    const last = customCompletionItems.length - 1;
    customCompletionIndex = Math.max(0, Math.min(customCompletionIndex + delta, last));
    customCompletionMenu
      .querySelectorAll(".cm-completionLabel")
      .forEach((row, index) => row.classList.toggle("selected", index === customCompletionIndex));
    return true;
  }

  // Highest precedence so these run before the default keymap (Enter would
  // otherwise insert a newline and the arrows move the caret off the `[[`).
  const customCompletionKeys = Prec.highest(
    keymap.of([
      {
        key: "Escape",
        run: () => {
          if (!customCompletionMenu) return false;
          closeCustomCompletion();
          return true;
        },
      },
      { key: "ArrowDown", run: () => moveCustomCompletion(1) },
      { key: "ArrowUp", run: () => moveCustomCompletion(-1) },
      {
        key: "Enter",
        run: () => {
          const item = customCompletionMenu && customCompletionItems[customCompletionIndex];
          if (!item) return false;
          applyCustomCompletion(item);
          return true;
        },
      },
    ]),
  );

  function modeExtensions(m: MarkdownViewMode): Extension {
    if (m === "source") return [];
    if (m === "reading")
      return [
        EditorState.readOnly.of(true),
        EditorView.editable.of(false),
        EditorView.contentAttributes.of({
          contenteditable: "false",
          inputmode: "none",
          "aria-readonly": "true",
        }),
      ];
    return [];
  }

  const modeCompartment = new Compartment();
  const editableCompartment = new Compartment();

  const state = EditorState.create({
    doc: "",
    extensions: [
      highlightSpecialChars(),
      history(),
      dropCursor(),
      indentOnInput(),
      syntaxHighlighting(noteHighlightStyle),
      bracketMatching(),
      markdown({ codeLanguages: languages }),
      customCompletionKeys,
      keymap.of([
        {
          key: "Tab",
          run: (v) => {
            const { from, to } = v.state.selection.main;
            const currentLine = v.state.doc.lineAt(from);
            const line = currentLine.text;
            const localFrom = from - currentLine.from;
            const localTo = to - currentLine.from;
            const link = /\[([^\]]+)\]\((https?:\/\/[^)]*)\)/.exec(line);
            if (link && localFrom >= link.index && localTo <= link.index + link[0].length) {
              const close = line.indexOf(")", link.index);
              if (close >= 0) {
                const pos = currentLine.from + close;
                v.dispatch({ selection: { anchor: pos } });
                return true;
              }
            }
            return indentMore(v);
          },
        },
        ...defaultKeymap,
        ...historyKeymap,
        { key: "Shift-Tab", run: indentLess },
      ]),
      EditorView.lineWrapping,
      // The top bar and the floating format toolbar overlay the scroller; keep the
      // caret clear of them when CodeMirror scrolls it into view (e.g. while typing).
      EditorView.scrollMargins.of((v) => {
        const box = v.scrollDOM.getBoundingClientRect();
        const bar = document.querySelector(".top-controls")?.getBoundingClientRect();
        const pill = document
          .querySelector(".toolbar-anchor .format-pill")
          ?.getBoundingClientRect();
        return {
          top: bar ? Math.max(0, bar.bottom - box.top) : 0,
          bottom: pill ? Math.max(0, box.bottom - pill.top + 12) : 0,
        };
      }),
      EditorView.contentAttributes.of({
        "aria-label": "Editor Markdown",
        spellcheck: "true",
        autocorrect: "on",
        autocapitalize: "sentences",
      }),
      /Android/i.test(navigator.userAgent) ? capitalizeAfterTask : [],
      autocompletion({ override: [tagCompletion] }),
      livePreviewField,
      cb.onRenameTitle ? inlineTitle(cb.title ?? "", cb.onRenameTitle) : [],
      linkClick,
      modeCompartment.of(modeExtensions(initialMode)),
      editableCompartment.of(EditorView.editable.of(initialMode !== "reading")),
      EditorView.updateListener.of((update) => {
        if (update.selectionSet)
          lastValidSelection = {
            anchor: update.state.selection.main.anchor,
            head: update.state.selection.main.head,
          };
        if (update.docChanged && !applyingExternal) cb.onChange();
        if (update.docChanged || update.selectionSet)
          requestAnimationFrame(refreshCustomCompletion);
      }),
    ],
  });

  const view = new EditorView({ state, parent: host });

  function revealCaret(): void {
    const head = view.state.selection.main.head;
    view.requestMeasure();
    view.dispatch({ effects: EditorView.scrollIntoView(head, { y: "nearest", yMargin: 36 }) });
  }
  let lastVvHeight = window.visualViewport?.height ?? window.innerHeight;
  function onVvResize(): void {
    updateViewport();
    const h = window.visualViewport?.height ?? window.innerHeight;
    // Only when the text itself has focus, not a field inside it (e.g. the inline title).
    if (lastVvHeight - h >= 40 && view.hasFocus) scheduleCaretReveal(revealCaret, 320);
    lastVvHeight = h;
  }
  function onFocus(e: FocusEvent): void {
    if (e.target !== view.contentDOM) return;
    scheduleCaretReveal(revealCaret, 320);
  }
  view.dom.addEventListener("focus", onFocus, true);
  view.scrollDOM.addEventListener("scroll", positionCustomCompletion, { passive: true });
  window.visualViewport?.addEventListener("resize", onVvResize);
  window.visualViewport?.addEventListener("scroll", updateViewport);

  function replaceDocumentAndSelect(
    text: string,
    nextSelection: { anchor: number; head?: number },
    after?: () => void,
  ): void {
    // Replace only the span that changed: swapping the whole document loses the
    // scroll position, so the view jumped away from the edit.
    const old = view.state.doc.toString();
    let start = 0;
    const max = Math.min(old.length, text.length);
    while (start < max && old.charCodeAt(start) === text.charCodeAt(start)) start++;
    let oldEnd = old.length;
    let newEnd = text.length;
    while (
      oldEnd > start &&
      newEnd > start &&
      old.charCodeAt(oldEnd - 1) === text.charCodeAt(newEnd - 1)
    ) {
      oldEnd--;
      newEnd--;
    }
    view.focus();
    view.dispatch({
      changes: { from: start, to: oldEnd, insert: text.slice(start, newEnd) },
      selection: nextSelection,
      scrollIntoView: true,
    });
    after?.();
  }

  function toggleInline(marker: string): void {
    const { from, to } = selection();
    const edit = wrapMarkdownSelection(view.state.doc.toString(), from, to, marker);
    replaceDocumentAndSelect(edit.doc, { anchor: edit.anchor, head: edit.head });
  }

  function eachSelectedLine(
    fn: (
      line: { from: number; to: number; text: string },
      index: number,
    ) => { from: number; to: number; insert: string },
  ): void {
    const { state: st } = view;
    const { from, to } = selection();
    const startLine = st.doc.lineAt(from);
    const endLine = st.doc.lineAt(to);
    const specs: { from: number; to: number; insert: string }[] = [];
    for (let i = startLine.number; i <= endLine.number; i++) {
      const line = st.doc.line(i);
      specs.push(fn(line, i));
    }
    const changes = st.changes(specs);
    view.focus();
    // assoc 1: a caret at the insertion point lands after the new prefix, not before it.
    view.dispatch({ changes, selection: st.selection.map(changes, 1), scrollIntoView: true });
  }

  const LIST_STRIP = /^(#{1,6}\s+|-\s\[[ xX]\]\s+|[-*+]\s+|\d+[.)]\s+|>\s?)+/;

  function applyHeading(level: number): void {
    const prefix = level > 0 ? "#".repeat(level) + " " : "";
    const { state: st } = view;
    const { from, to } = selection();
    const startLine = st.doc.lineAt(from);
    const endLine = st.doc.lineAt(to);
    let allAt = level > 0;
    for (let i = startLine.number; i <= endLine.number; i++) {
      const t = st.doc.line(i).text;
      const at = level > 0 ? t.startsWith(prefix) : !/^(#{1,6}\s+)/.test(t);
      if (!at) {
        allAt = false;
        break;
      }
    }
    eachSelectedLine((line) => {
      // Touch only the prefix so the caret keeps its place in the line text.
      const oldPrefix = LIST_STRIP.exec(line.text)?.[0] ?? "";
      return { from: line.from, to: line.from + oldPrefix.length, insert: allAt ? "" : prefix };
    });
  }

  function toggleBlockPrefix(prefix: string): void {
    const { from, to } = selection();
    const edit = toggleMarkdownPrefix(view.state.doc.toString(), from, to, prefix);
    replaceDocumentAndSelect(edit.doc, { anchor: edit.anchor, head: edit.head });
  }

  function insertBlock(text: string, selectOffset: number): void {
    const { from, to } = selection();
    const current = view.state.doc.toString();
    replaceDocumentAndSelect(current.slice(0, from) + text + current.slice(to), {
      anchor: from + selectOffset,
    });
  }

  function indentLines(increase: boolean): void {
    const { state: st } = view;
    const { from, to } = selection();
    const startLine = st.doc.lineAt(from);
    const endLine = st.doc.lineAt(to);
    const changes: { from: number; to: number; insert: string }[] = [];
    for (let i = startLine.number; i <= endLine.number; i++) {
      const line = st.doc.line(i);
      if (increase) {
        changes.push({ from: line.from, to: line.from, insert: "  " });
      } else {
        const lead = /^ {1,2}/.exec(line.text);
        if (lead) changes.push({ from: line.from, to: line.from + lead[0].length, insert: "" });
      }
    }
    view.focus();
    view.dispatch({ changes, scrollIntoView: true });
  }

  function applyTableOp(op: MarkdownTableOperation): void {
    const edit = editMarkdownTable(view.state.doc.toString(), selection().from, op);
    if (!edit) return;
    replaceDocumentAndSelect(edit.doc, { anchor: edit.anchor, head: edit.head });
  }

  const fmt: FormattingEngine = {
    headingCycle: () => {
      const { state: st } = view;
      const line = st.doc.lineAt(selection().to);
      const m = /^(#{1,6})\s+/.exec(line.text);
      if (m) {
        const level = m[1].length;
        const next = level >= 3 ? 0 : level + 1;
        applyHeading(next);
      } else {
        applyHeading(1);
      }
    },
    bold: () => toggleInline("**"),
    italic: () => toggleInline("*"),
    strike: () => toggleInline("~~"),
    highlight: () => toggleInline("=="),
    code: () => toggleInline("`"),
    bulletList: () => toggleBlockPrefix("- "),
    orderedList: () => toggleBlockPrefix("1. "),
    taskList: () => toggleBlockPrefix("- [ ] "),
    blockquote: () => toggleBlockPrefix("> "),
    indent: () => indentLines(true),
    outdent: () => indentLines(false),
    codeBlock: () => {
      const { from, to } = selection();
      const sel = view.state.sliceDoc(from, to);
      const block = "```\n" + sel + "\n```\n";
      const current = view.state.doc.toString();
      replaceDocumentAndSelect(current.slice(0, from) + block + current.slice(to), {
        anchor: from + (sel ? block.length - 4 : 4),
      });
    },
    horizontalRule: () => insertBlock("\n\n---\n\n", 5),
    insertTable: () => {
      const table =
        "| Columna A | Columna B | Columna C |\n| --- | --- | --- |\n|  |  |  |\n|  |  |  |\n|  |  |  |\n";
      insertBlock("\n\n" + table, 2);
    },
    addMarkdownRow: () => applyTableOp("add-row"),
    addMarkdownColumn: () => applyTableOp("add-column"),
    deleteMarkdownRow: () => applyTableOp("delete-row"),
    deleteMarkdownColumn: () => applyTableOp("delete-column"),
    deleteMarkdownTable: () => applyTableOp("delete-table"),
    noteLink: () => {
      const { from, to } = selection();
      const edit = insertMarkdown(view.state.doc.toString(), from, to, "[[]]", 2);
      replaceDocumentAndSelect(
        edit.doc,
        { anchor: edit.anchor },
        () => void showCustomCompletion("", edit.anchor, edit.anchor, "[["),
      );
    },
    embed: () => {
      const { from, to } = selection();
      const edit = insertMarkdown(view.state.doc.toString(), from, to, "![[]]", 3);
      replaceDocumentAndSelect(
        edit.doc,
        { anchor: edit.anchor },
        () => void showCustomCompletion("", edit.anchor, edit.anchor, "![["),
      );
    },
    tag: () => {
      const { from, to } = selection();
      const current = view.state.doc.toString();
      replaceDocumentAndSelect(current.slice(0, from) + "#" + current.slice(to), {
        anchor: from + 1,
      });
    },
    attachment: () => window.dispatchEvent(new CustomEvent("notium:attachment-menu")),
    externalLink: () => {
      const { from, to } = selection();
      const sel = view.state.sliceDoc(from, to);
      if (sel) {
        const current = view.state.doc.toString();
        replaceDocumentAndSelect(
          current.slice(0, from) + `[${sel}](https://)` + current.slice(to),
          { anchor: from + sel.length + 3, head: from + sel.length + 11 },
        );
      } else {
        const current = view.state.doc.toString();
        replaceDocumentAndSelect(current.slice(0, from) + "[texto](https://)" + current.slice(to), {
          anchor: from + 1,
          head: from + 6,
        });
      }
    },
    undo: () => undo(view),
    redo: () => redo(view),
    setHeading: (level) => applyHeading(level),
    wrap: (marker) => toggleInline(marker),
    insertText: (text, cursorOffset) => insertBlock(text, cursorOffset),
    exec: (cmd) => void cmd(view),
  };

  return {
    setContent(text: string) {
      if (view.state.doc.toString() === text) return;
      applyingExternal = true;
      try {
        view.dispatch({
          changes: { from: 0, to: view.state.doc.length, insert: text },
          selection: { anchor: 0 },
          // Not undoable: undoing a load would empty the note and save it.
          annotations: Transaction.addToHistory.of(false),
        });
        requestAnimationFrame(() => view.requestMeasure());
      } finally {
        applyingExternal = false;
      }
    },
    getContent() {
      return view.state.doc.toString();
    },
    focus() {
      view.focus();
    },
    blur() {
      view.contentDOM.blur();
    },
    getPlainText() {
      return mdToPlainText(view.state.doc.toString());
    },
    destroy() {
      destroyed = true;
      closeCustomCompletion();
      view.dom.removeEventListener("focus", onFocus, true);
      view.scrollDOM.removeEventListener("scroll", positionCustomCompletion);
      window.visualViewport?.removeEventListener("resize", onVvResize);
      window.visualViewport?.removeEventListener("scroll", updateViewport);
      view.destroy();
    },
    setMode(m: MarkdownViewMode) {
      mode = m;
      if (m !== "live") closeCustomCompletion();
      view.dispatch({
        effects: [
          forceDecorationsEffect.of(null),
          modeCompartment.reconfigure(modeExtensions(m)),
          editableCompartment.reconfigure(EditorView.editable.of(m !== "reading")),
        ],
      });
      requestAnimationFrame(() => view.requestMeasure());
    },
    getMode() {
      return mode;
    },
    insertAttachment(path: string) {
      insertBlock(`![[${path}]]`, path.length + 5);
    },
    scrollToHeading(heading: string): boolean {
      const target = normalizeHeading(heading);
      if (!target) return false;
      const doc = view.state.doc;
      for (let i = 1; i <= doc.lines; i++) {
        const line = doc.line(i);
        const m = /^(#{1,6})\s+(.+)$/.exec(line.text);
        if (m && normalizeHeading(m[2]) === target) {
          view.dispatch({
            selection: { anchor: line.from },
            effects: EditorView.scrollIntoView(line.from, { y: "start", yMargin: 64 }),
          });
          requestAnimationFrame(() => {
            try {
              const block = view.lineBlockAt(line.from);
              view.scrollDOM.scrollTo({
                top: Math.max(0, block.top - 64),
                behavior: "smooth",
              });
            } catch {
              // fallback handled by EditorView.scrollIntoView
            }
          });
          return true;
        }
      }
      return false;
    },
    ...fmt,
  };
}
