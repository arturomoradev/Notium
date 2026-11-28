// Tiptap schema for Notium notes (stored as Tiptap JSON): StarterKit plus the
// Obsidian-style links Notium uses. Kept free of DOM code so tests run under Node.
import { Node, mergeAttributes, type AnyExtension } from "@tiptap/core";
import StarterKit from "@tiptap/starter-kit";
import { TaskItem, TaskList } from "@tiptap/extension-list";
import { TableKit } from "@tiptap/extension-table";
import Highlight from "@tiptap/extension-highlight";
import CodeBlockLowlight from "@tiptap/extension-code-block-lowlight";
import Mathematics from "@tiptap/extension-mathematics";
import { common, createLowlight } from "lowlight";

/** Highlight.js grammars for the common languages (JS/TS, Python, Rust, SQL, shell…). */
export const lowlight = createLowlight(common);

/** Callout types, as in Obsidian; unknown types render like `note`. */
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
  example: { icon: "📋", label: "Ejemplo", kind: "info" },
  quote: { icon: "💬", label: "Cita", kind: "quote" },
};

/** `> [!type]± title` callout: a titled box around blocks; `fold` is "", "+" or "-". */
export const Callout = Node.create({
  name: "callout",
  group: "block",
  content: "block+",
  defining: true,

  addAttributes() {
    return {
      type: { default: "note" },
      title: { default: "" },
      fold: { default: "" },
    };
  },

  parseHTML() {
    return [
      {
        tag: "div[data-callout]",
        getAttrs: (el) => ({
          type: (el as HTMLElement).dataset.callout ?? "note",
          title: (el as HTMLElement).dataset.title ?? "",
          fold: (el as HTMLElement).dataset.fold ?? "",
        }),
      },
    ];
  },

  renderHTML({ node, HTMLAttributes }) {
    const { type, title, fold } = node.attrs as { type: string; title: string; fold: string };
    return [
      "div",
      mergeAttributes(HTMLAttributes, {
        "data-callout": type,
        "data-title": title,
        "data-fold": fold,
      }),
      0,
    ];
  },
});

/** `[[target]]` (link) and `![[target]]` (embed), stored verbatim as `target`. */
export const WikiLink = Node.create({
  name: "wikiLink",
  group: "inline",
  inline: true,
  atom: true,
  selectable: true,

  addAttributes() {
    return {
      target: { default: "" },
      embed: { default: false },
    };
  },

  parseHTML() {
    return [
      {
        tag: "span[data-wiki]",
        getAttrs: (el) => ({
          target: (el as HTMLElement).dataset.wiki ?? "",
          embed: (el as HTMLElement).dataset.embed === "true",
        }),
      },
    ];
  },

  renderHTML({ node, HTMLAttributes }) {
    const { target, embed } = node.attrs as { target: string; embed: boolean };
    return [
      "span",
      mergeAttributes(HTMLAttributes, {
        "data-wiki": target,
        "data-embed": String(embed),
        class: embed ? "tt-wiki-embed" : "tt-wiki-link",
      }),
      `${embed ? "!" : ""}[[${target}]]`,
    ];
  },

  renderText({ node }) {
    return `${node.attrs.embed ? "!" : ""}[[${node.attrs.target}]]`;
  },
});

/** Every extension except DOM-only behaviour (completion, keyboard tweaks). */
export function noteExtensions(): AnyExtension[] {
  return [
    StarterKit.configure({
      link: { openOnClick: false, autolink: true },
      underline: false,
      codeBlock: false,
    }),
    // Separate so the editor can give it a header and a Mermaid view (noteEngine.ts).
    CodeBlockLowlight.configure({ lowlight }),
    Callout,
    TaskList,
    TaskItem.configure({ nested: true }),
    TableKit.configure({ table: { resizable: false } }),
    Highlight,
    WikiLink,
    Mathematics.configure({ katexOptions: { throwOnError: false } }),
  ];
}
