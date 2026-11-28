// Tiptap engine for Notium notes: WYSIWYG editing, the Tiptap doc (JSON) on disk.
import { Editor, Extension, nodeInputRule } from "@tiptap/core";
import { PluginKey } from "@tiptap/pm/state";
import Suggestion, { exitSuggestion, type SuggestionProps } from "@tiptap/suggestion";
import { CharacterCount } from "@tiptap/extensions";
import type { EditorEngine, EditorEngineCallbacks } from "./engine";
import type { FormattingEngine } from "./toolbar";
import { scheduleCaretReveal, updateViewport } from "../../lib/mobile/viewport";
import { readAttachment } from "../../lib/ipc/commands";
import {
  attachmentUrlCache,
  buildWikiCompletionEntries,
  imageMime,
  invalidateEmbedCache,
  normalizeHeading,
  type WikiCompletionEntry,
} from "./widgets/caches";
import { CALLOUT_META, Callout, WikiLink, lowlight, noteExtensions } from "./tiptapExtensions";
import CodeBlockLowlight from "@tiptap/extension-code-block-lowlight";
import Mathematics from "@tiptap/extension-mathematics";
import type { Node as PMNode } from "@tiptap/pm/model";
import "katex/dist/katex.min.css";
import { migrateNote } from "./noteMigration";

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
      // Errors are shown in the block; don't let Mermaid append its error SVG to <body>.
      suppressErrorRendering: true,
    });
    mermaidTheme = theme;
  }
  const { svg } = await mermaid.render(`notium-mermaid-${++mermaidSeq}`, code);
  return svg;
}

export { invalidateEmbedCache };
export type EditorViewMode = "live" | "source" | "reading";

export interface NoteEngine extends EditorEngine, FormattingEngine {
  setMode(mode: EditorViewMode): void;
  getMode(): EditorViewMode;
  blur(): void;
  getStats(): { words: number; chars: number };
  insertAttachment(path: string, mime: string): void;
  scrollToHeading(heading: string): boolean;
}

const IMAGE = /\.(png|jpe?g|gif|webp|svg)$/i;

function imageElement(path: string): HTMLImageElement {
  const image = document.createElement("img");
  image.className = "cm-vault-image";
  image.alt = path.split("/").pop() ?? "Adjunto";
  image.decoding = "async";
  let url = attachmentUrlCache.get(path);
  if (!url) {
    url = readAttachment(path).then((bytes) =>
      URL.createObjectURL(new Blob([bytes], { type: imageMime(path) })),
    );
    attachmentUrlCache.set(path, url);
    // A failed read must not stay cached, so the next render retries.
    url.catch(() => attachmentUrlCache.delete(path));
  }
  void url.then((u) => (image.src = u)).catch(() => {});
  return image;
}

export function mountEditor(
  host: HTMLElement,
  cb: EditorEngineCallbacks,
  initialMode: EditorViewMode = "live",
): NoteEngine {
  let mode: EditorViewMode = initialMode;
  let loaded = "";
  let dirty = false;
  let applyingExternal = false;
  let destroyed = false;

  const scroller = document.createElement("div");
  scroller.className = "tt-scroller";
  const column = document.createElement("div");
  column.className = "tt-column";
  scroller.appendChild(column);
  host.appendChild(scroller);

  if (cb.onRenameTitle) column.appendChild(titleField(cb.title ?? "", cb.onRenameTitle));
  const mount = document.createElement("div");
  column.appendChild(mount);

  // `[[x]]` typed by hand becomes a link node as soon as it is closed.
  const LiveWikiLink = WikiLink.extend({
    addInputRules() {
      return [
        nodeInputRule({
          find: /(!?)\[\[([^\]\n]+)\]\]$/,
          type: this.type,
          getAttributes: (m) => ({ embed: m[1] === "!", target: m[2] }),
        }),
      ];
    },
    addNodeView() {
      return ({ node }) => {
        const { target, embed } = node.attrs as { target: string; embed: boolean };
        if (embed && IMAGE.test(target)) {
          const dom = document.createElement("span");
          dom.className = "tt-wiki-embed";
          dom.appendChild(imageElement(target));
          return { dom };
        }
        const dom = document.createElement("span");
        dom.className = embed ? "cm-wiki-link-pill tt-wiki-embed" : "cm-wiki-link";
        dom.textContent = embed ? `↳ ${target}` : target.split("|").pop()!;
        dom.title = target;
        dom.addEventListener("mousedown", (e) => {
          if (e.button !== 0 || embed) return;
          e.preventDefault();
          cb.onLinkActivate(target.trim());
        });
        return { dom };
      };
    },
  });

  // `mermaid` code blocks show the diagram; the source appears while the caret is inside.
  const MermaidCodeBlock = CodeBlockLowlight.extend({
    addNodeView() {
      return ({ node: initial, getPos, editor: ed }) => {
        let node = initial;
        const isMermaid = (n: PMNode) => n.attrs.language === "mermaid";
        const pre = document.createElement("pre");
        const code = document.createElement("code");
        pre.appendChild(code);
        if (node.attrs.language) code.className = `language-${node.attrs.language}`;
        if (!isMermaid(node)) {
          // Language label + copy button, as on main; lowlight colours the code.
          const dom = document.createElement("div");
          dom.className = "cm-codeblock-widget tt-code";
          const head = document.createElement("div");
          head.className = "tt-code-head";
          head.contentEditable = "false";
          const lang = document.createElement("span");
          lang.className = "cm-block-badge";
          lang.textContent = node.attrs.language || "Código";
          const copy = document.createElement("button");
          copy.type = "button";
          copy.className = "cm-callout-edit";
          copy.textContent = "Copiar";
          copy.addEventListener("mousedown", (e) => e.preventDefault());
          copy.addEventListener("click", () => {
            void navigator.clipboard
              ?.writeText(node.textContent)
              .then(() => {
                copy.textContent = "Copiado";
                setTimeout(() => (copy.textContent = "Copiar"), 1200);
              })
              .catch(() => {});
          });
          head.append(lang, copy);
          dom.append(head, pre);
          return {
            dom,
            contentDOM: code,
            update: (next) => {
              if (next.type !== node.type || next.attrs.language !== node.attrs.language)
                return false;
              node = next;
              return true;
            },
            ignoreMutation: (m) => !code.contains(m.target),
            stopEvent: (e) => head.contains(e.target as Node),
          };
        }

        const dom = document.createElement("div");
        dom.className = "cm-mermaid-widget tt-mermaid";
        const badge = document.createElement("div");
        badge.className = "cm-block-badge";
        badge.textContent = "Mermaid";
        badge.contentEditable = "false";
        const preview = document.createElement("div");
        preview.className = "cm-mermaid-diagram";
        preview.contentEditable = "false";
        dom.append(badge, preview, pre);

        let rendered = "";
        let token = 0;
        const render = () => {
          const src = node.textContent;
          if (src === rendered) return;
          rendered = src;
          const t = ++token;
          renderMermaid(src)
            .then((svg) => {
              if (t !== token) return;
              // Sanitized by Mermaid itself (securityLevel "strict" runs DOMPurify).
              preview.innerHTML = svg;
              preview.classList.remove("cm-mermaid-error");
            })
            .catch((e: unknown) => {
              if (t !== token) return;
              preview.textContent = e instanceof Error ? e.message : String(e);
              preview.classList.add("cm-mermaid-error");
            });
        };
        const syncEditing = () => {
          const pos = getPos();
          const { from, to } = ed.state.selection;
          const inside = pos != null && from > pos && to < pos + node.nodeSize;
          dom.classList.toggle("editing", inside && ed.isEditable);
        };
        preview.addEventListener("mousedown", (e) => {
          const pos = getPos();
          if (pos == null || !ed.isEditable) return;
          e.preventDefault();
          ed.chain()
            .focus()
            .setTextSelection(pos + 1)
            .run();
        });
        ed.on("selectionUpdate", syncEditing);
        render();
        return {
          dom,
          contentDOM: code,
          update: (next) => {
            if (next.type !== node.type || !isMermaid(next)) return false;
            node = next;
            render();
            return true;
          },
          ignoreMutation: (m) => !code.contains(m.target),
          destroy: () => ed.off("selectionUpdate", syncEditing),
        };
      };
    },
  });

  // shortcut: LaTeX is edited with the native prompt; swap for an inline field if Android drops it.
  const editLatex = (kind: "inline" | "block") => (node: PMNode, pos: number) => {
    if (!editor.isEditable) return;
    const latex = window.prompt("Fórmula LaTeX", node.attrs.latex as string);
    if (latex === null) return;
    const c = editor.chain().setNodeSelection(pos);
    if (!latex.trim()) c.deleteSelection().run();
    else if (kind === "inline") c.updateInlineMath({ latex }).run();
    else c.updateBlockMath({ latex }).run();
  };

  // Callout box: icon + title (click to rename), type picker, fold chevron.
  const CalloutView = Callout.extend({
    addNodeView() {
      return ({ node: initial, getPos, editor: ed }) => {
        let node = initial;
        let collapsed = node.attrs.fold === "-";
        const dom = document.createElement("div");
        const head = document.createElement("div");
        head.contentEditable = "false";
        const icon = document.createElement("span");
        icon.className = "cm-callout-icon";
        const title = document.createElement("span");
        title.className = "cm-callout-title-text";
        const picker = document.createElement("select");
        picker.className = "tt-callout-type";
        picker.setAttribute("aria-label", "Tipo de callout");
        for (const [value, meta] of Object.entries(CALLOUT_META))
          picker.add(new Option(`${meta.icon} ${meta.label}`, value));
        const chevron = document.createElement("span");
        chevron.className = "cm-callout-fold";
        head.append(icon, title, picker, chevron);
        const body = document.createElement("div");
        body.className = "cm-callout-body";
        dom.append(head, body);

        const paint = () => {
          const type = String(node.attrs.type).toLowerCase();
          const meta = CALLOUT_META[type] ?? { icon: "📝", label: type, kind: "note" };
          const fold = Boolean(node.attrs.fold);
          dom.className = `cm-callout cm-callout-${meta.kind} tt-callout${collapsed ? " is-collapsed" : ""}`;
          head.className = `cm-callout-title${fold ? " is-foldable" : ""}`;
          icon.textContent = meta.icon;
          title.textContent = String(node.attrs.title) || meta.label;
          picker.value = type in CALLOUT_META ? type : "note";
          chevron.hidden = !fold;
          chevron.textContent = collapsed ? "▸" : "▾";
        };
        const setAttrs = (attrs: Record<string, string>) => {
          const pos = getPos();
          if (pos == null || !ed.isEditable) return;
          ed.view.dispatch(ed.state.tr.setNodeMarkup(pos, undefined, { ...node.attrs, ...attrs }));
        };
        picker.addEventListener("change", () => setAttrs({ type: picker.value }));
        // shortcut: native prompt for the title, like links and formulas.
        title.addEventListener("click", () => {
          if (!ed.isEditable) return;
          const next = window.prompt("Título del callout", String(node.attrs.title));
          if (next !== null) setAttrs({ title: next.trim() });
        });
        chevron.addEventListener("click", () => {
          collapsed = !collapsed;
          paint();
        });
        paint();
        return {
          dom,
          contentDOM: body,
          update: (next) => {
            if (next.type !== node.type) return false;
            node = next;
            paint();
            return true;
          },
          stopEvent: (e) => head.contains(e.target as Node),
          ignoreMutation: (m) => !body.contains(m.target),
        };
      };
    },
  });

  // `[[` / `![[` note-link menu, on Tiptap's official suggestion utility.
  const wikiKey = new PluginKey("wikiSuggestion");
  const WikiSuggestion = Extension.create({
    name: "wikiSuggestion",
    addProseMirrorPlugins() {
      return [
        Suggestion<WikiCompletionEntry, WikiCompletionEntry>({
          editor: this.editor,
          pluginKey: wikiKey,
          char: "[[",
          allowSpaces: true, // note titles have spaces
          allowedPrefixes: null, // after `!` (embeds), `(`, etc.
          floatingUi: { strategy: "fixed" },
          allow: ({ editor: ed }) => mode === "live" && ed.isEditable,
          items: ({ query }) => buildWikiCompletionEntries(query, getContent()),
          command: ({ editor: ed, range, props: item }) => {
            const embed =
              range.from > 0 && ed.state.doc.textBetween(range.from - 1, range.from) === "!";
            const from = embed ? range.from - 1 : range.from;
            const prefix = embed ? "![[" : "[[";
            if (item.keepOpenForSection) {
              ed.chain()
                .focus()
                .insertContentAt({ from, to: range.to }, prefix + item.insertValue)
                .run();
              return;
            }
            const size = ed.state.doc.content.size;
            const after = ed.state.doc.textBetween(range.to, Math.min(range.to + 2, size));
            ed.chain()
              .focus()
              .insertContentAt(
                { from, to: after === "]]" ? range.to + 2 : range.to },
                { type: "wikiLink", attrs: { target: item.insertValue, embed } },
              )
              .run();
          },
          render: () => {
            const menu = document.createElement("div");
            menu.className = "cm-tooltip cm-tooltip-autocomplete cm-wiki-completion";
            menu.setAttribute("role", "listbox");
            let unmount: (() => void) | undefined;
            let current: SuggestionProps<WikiCompletionEntry, WikiCompletionEntry> | undefined;
            let index = 0;
            const paint = () => {
              if (!current) return;
              const { items, query, loading } = current;
              menu.textContent = "";
              if (!items.length && !loading) {
                const empty = document.createElement("div");
                empty.className = "cm-completionEmpty";
                empty.textContent = query.includes("#")
                  ? "No hay secciones coincidentes"
                  : "No hay notas coincidentes";
                menu.appendChild(empty);
              }
              items.forEach((item, i) => {
                const row = document.createElement("button");
                row.type = "button";
                row.className = "cm-completionLabel" + (i === index ? " selected" : "");
                row.textContent = item.label;
                row.addEventListener("mousedown", (e) => {
                  e.preventDefault();
                  current?.command(item);
                });
                menu.appendChild(row);
              });
            };
            return {
              onStart: (props) => {
                current = props;
                index = 0;
                paint();
                unmount = props.mount(menu);
              },
              onUpdate: (props) => {
                current = props;
                index = Math.min(index, Math.max(0, props.items.length - 1));
                paint();
              },
              onKeyDown: ({ event, view }) => {
                const items = current?.items ?? [];
                if (event.key === "Escape") {
                  exitSuggestion(view, wikiKey);
                  return true;
                }
                if (event.key === "ArrowDown" || event.key === "ArrowUp") {
                  const step = event.key === "ArrowDown" ? 1 : -1;
                  index = Math.max(0, Math.min(index + step, items.length - 1));
                  paint();
                  return true;
                }
                if (event.key === "Enter" && items[index]) {
                  current?.command(items[index]);
                  return true;
                }
                return false;
              },
              onExit: () => {
                unmount?.();
                current = undefined;
              },
            };
          },
        }),
      ];
    },
  });
  const exitWiki = () => exitSuggestion(editor.view, wikiKey);

  const replaced = new Set(["wikiLink", "codeBlock", "callout", "Mathematics"]);
  const editor = new Editor({
    element: mount,
    extensions: [
      ...noteExtensions().filter((e) => !replaced.has(e.name)),
      LiveWikiLink,
      MermaidCodeBlock.configure({ lowlight }),
      CalloutView,
      WikiSuggestion,
      CharacterCount,
      Mathematics.configure({
        katexOptions: { throwOnError: false },
        inlineOptions: { onClick: editLatex("inline") },
        blockOptions: { onClick: editLatex("block") },
      }),
    ],
    editable: initialMode !== "reading",
    editorProps: {
      attributes: {
        class: "tt-content",
        "aria-label": "Editor de notas",
        spellcheck: "true",
        autocorrect: "on",
        autocapitalize: "sentences",
      },
    },
    onUpdate: () => {
      if (applyingExternal) return;
      dirty = true;
      cb.onChange();
    },
  });

  // ---------- mobile keyboard ----------
  const revealCaret = () => editor.commands.scrollIntoView();
  let lastVvHeight = window.visualViewport?.height ?? window.innerHeight;
  function onVvResize(): void {
    updateViewport();
    const h = window.visualViewport?.height ?? window.innerHeight;
    if (lastVvHeight - h >= 40 && editor.isFocused) scheduleCaretReveal(revealCaret, 320);
    lastVvHeight = h;
  }
  const onFocus = () => scheduleCaretReveal(revealCaret, 320);
  editor.on("focus", onFocus);
  window.visualViewport?.addEventListener("resize", onVvResize);
  window.visualViewport?.addEventListener("scroll", updateViewport);

  function getContent(): string {
    // Untouched notes are returned verbatim, so opening one never rewrites it.
    return dirty ? JSON.stringify(editor.getJSON()) : loaded;
  }

  const chain = () => editor.chain().focus();
  const inList = () => (editor.isActive("taskItem") ? "taskItem" : "listItem");

  function headingLevel(): number {
    return editor.isActive("heading") ? (editor.getAttributes("heading").level as number) : 0;
  }

  function setHeading(level: number): void {
    if (level === 0) chain().setParagraph().run();
    else
      chain()
        .setHeading({ level: level as 1 | 2 | 3 | 4 | 5 | 6 })
        .run();
  }

  const fmt: FormattingEngine = {
    undo: () => chain().undo().run(),
    redo: () => chain().redo().run(),
    noteLink: () => chain().insertContent("[[").run(),
    embed: () => chain().insertContent("![[").run(),
    tag: () => chain().insertContent("#").run(),
    attachment: () => window.dispatchEvent(new CustomEvent("notium:attachment-menu")),
    headingCycle: () => {
      const level = headingLevel();
      setHeading(level >= 3 ? 0 : level + 1);
    },
    bold: () => chain().toggleBold().run(),
    italic: () => chain().toggleItalic().run(),
    strike: () => chain().toggleStrike().run(),
    highlight: () => chain().toggleHighlight().run(),
    code: () => chain().toggleCode().run(),
    blockquote: () => chain().toggleBlockquote().run(),
    externalLink: () => {
      const current = (editor.getAttributes("link").href as string | undefined) ?? "https://";
      // shortcut: native prompt; swap for an in-app popover if Android drops it.
      const href = window.prompt("URL del enlace", current);
      if (href === null) return;
      if (!href.trim()) return void chain().unsetLink().run();
      if (editor.state.selection.empty && !editor.isActive("link"))
        chain()
          .insertContent({ type: "text", text: href, marks: [{ type: "link", attrs: { href } }] })
          .run();
      else chain().extendMarkRange("link").setLink({ href }).run();
    },
    bulletList: () => chain().toggleBulletList().run(),
    orderedList: () => chain().toggleOrderedList().run(),
    taskList: () => chain().toggleTaskList().run(),
    indent: () => chain().sinkListItem(inList()).run(),
    outdent: () => chain().liftListItem(inList()).run(),
    insertTable: () => chain().insertTable({ rows: 4, cols: 3, withHeaderRow: true }).run(),
    codeBlock: () => chain().toggleCodeBlock().run(),
    horizontalRule: () => chain().setHorizontalRule().run(),
    setHeading,
    wrap: (marker) => {
      const { from, to } = editor.state.selection;
      const text = editor.state.doc.textBetween(from, to, "\n");
      chain()
        .insertContentAt({ from, to }, marker + text + marker)
        .setTextSelection(from + marker.length + text.length)
        .run();
    },
    insertText: (text) => chain().insertContent(text).run(),
    exec: (cmd) => void cmd(editor),
    addMarkdownRow: () => chain().addRowAfter().run(),
    addMarkdownColumn: () => chain().addColumnAfter().run(),
    deleteMarkdownRow: () => chain().deleteRow().run(),
    deleteMarkdownColumn: () => chain().deleteColumn().run(),
    deleteMarkdownTable: () => chain().deleteTable().run(),
  };

  return {
    ...fmt,
    setContent(text: string) {
      if (text === getContent()) return;
      loaded = text;
      let doc: object | null = null;
      let migrated = false;
      try {
        doc = JSON.parse(text) as object;
        migrated = migrateNote(doc);
      } catch {
        // Shown empty; `loaded` is still what a save writes back until the user edits.
      }
      applyingExternal = true;
      try {
        // Not undoable: undoing a load would empty the note and save it.
        editor
          .chain()
          .setMeta("addToHistory", false)
          .setContent(doc ?? "", { emitUpdate: false })
          .setTextSelection(1)
          .run();
      } finally {
        applyingExternal = false;
        dirty = false;
      }
      // Formulas written as `$…$` text become math nodes once, and that is saved.
      if (migrated) {
        dirty = true;
        cb.onChange();
      }
    },
    getContent,
    focus: () => editor.commands.focus(),
    blur: () => editor.commands.blur(),
    getStats: () => ({
      words: editor.storage.characterCount.words(),
      chars: editor.storage.characterCount.characters(),
    }),
    destroy() {
      destroyed = true;
      exitWiki();
      window.visualViewport?.removeEventListener("resize", onVvResize);
      window.visualViewport?.removeEventListener("scroll", updateViewport);
      editor.destroy();
      scroller.remove();
    },
    setMode(m: EditorViewMode) {
      // shortcut: "source" shows the same WYSIWYG view; add a raw Markdown pane if it is missed.
      mode = m;
      if (m !== "live") exitWiki();
      editor.setEditable(m !== "reading");
    },
    getMode: () => mode,
    insertAttachment(path: string) {
      chain()
        .insertContent({ type: "wikiLink", attrs: { target: path, embed: true } })
        .run();
    },
    scrollToHeading(heading: string): boolean {
      const target = normalizeHeading(heading);
      if (!target) return false;
      let found = -1;
      editor.state.doc.descendants((node, pos) => {
        if (found >= 0) return false;
        if (node.type.name === "heading" && normalizeHeading(node.textContent) === target)
          found = pos;
      });
      if (found < 0) return false;
      editor.commands.setTextSelection(found + 1);
      const dom = editor.view.nodeDOM(found);
      if (dom instanceof HTMLElement) {
        scroller.scrollTo({
          top: Math.max(0, dom.offsetTop - 64),
          behavior: "smooth",
        });
      }
      return true;
    },
  };

  function titleField(title: string, onRename: (t: string) => Promise<boolean>) {
    const field = document.createElement("textarea");
    field.className = "cm-inline-title";
    field.rows = 1;
    field.value = title;
    field.spellcheck = false;
    field.setAttribute("aria-label", "Título de la nota");
    let current = title;
    field.addEventListener("keydown", (e) => {
      if (e.key === "Enter") {
        e.preventDefault();
        // Enter continues into the note body, like Obsidian.
        editor.commands.focus("start");
      } else if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        field.value = current;
        field.blur();
      }
    });
    field.addEventListener("blur", async () => {
      const next = field.value.replace(/\s+/g, " ").trim();
      if (!next || next === current) {
        field.value = current;
        return;
      }
      if (await onRename(next)) current = next;
      else field.value = current;
    });
    return field;
  }
}
