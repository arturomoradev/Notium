// Tiptap schema for Notium notes (stored as Tiptap JSON): StarterKit plus the
// Obsidian-style links Notium uses. Kept free of DOM code so tests run under Node.
import { Node, mergeAttributes, type AnyExtension } from "@tiptap/core";
import StarterKit from "@tiptap/starter-kit";
import { TaskItem, TaskList } from "@tiptap/extension-list";
import { TableKit } from "@tiptap/extension-table";
import Highlight from "@tiptap/extension-highlight";
import CodeBlock from "@tiptap/extension-code-block";
import Mathematics from "@tiptap/extension-mathematics";

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
    // Separate so the editor can give it a Mermaid view (noteEngine.ts).
    CodeBlock,
    TaskList,
    TaskItem.configure({ nested: true }),
    TableKit.configure({ table: { resizable: false } }),
    Highlight,
    WikiLink,
    Mathematics.configure({ katexOptions: { throwOnError: false } }),
  ];
}
