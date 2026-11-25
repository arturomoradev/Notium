import { HighlightStyle, LanguageDescription, defaultHighlightStyle } from "@codemirror/language";
import { languages } from "@codemirror/language-data";
import { highlightCode, tags as t } from "@lezer/highlight";

// Default markdown styling plus theme-aware code colours (later specs win per tag).
export const noteHighlightStyle = HighlightStyle.define([
  ...defaultHighlightStyle.specs,
  { tag: [t.keyword, t.controlKeyword, t.moduleKeyword, t.operatorKeyword], color: "var(--syn-keyword)" },
  { tag: [t.string, t.special(t.string), t.regexp], color: "var(--syn-string)" },
  { tag: [t.number, t.bool, t.null, t.atom], color: "var(--syn-number)" },
  { tag: [t.comment, t.lineComment, t.blockComment], color: "var(--syn-comment)", fontStyle: "italic" },
  { tag: [t.function(t.variableName), t.function(t.propertyName)], color: "var(--syn-function)" },
  { tag: [t.typeName, t.className, t.namespace], color: "var(--syn-type)" },
  { tag: [t.definition(t.variableName), t.propertyName, t.attributeName], color: "var(--syn-property)" },
  { tag: [t.tagName, t.meta], color: "var(--syn-tag)" },
  { tag: [t.operator, t.punctuation, t.bracket], color: "var(--syn-punct)" },
]);

/** Fills `el` with `code`, then swaps in highlighted spans once the language loads. */
export function renderHighlightedCode(el: HTMLElement, lang: string, code: string): void {
  el.textContent = code;
  const desc = lang ? LanguageDescription.matchLanguageName(languages, lang, true) : null;
  if (!desc) return;
  desc
    .load()
    .then((support) => {
      const frag = document.createDocumentFragment();
      highlightCode(
        code,
        support.language.parser.parse(code),
        noteHighlightStyle,
        (text, classes) => {
          if (!classes) return void frag.appendChild(document.createTextNode(text));
          const span = document.createElement("span");
          span.className = classes;
          span.textContent = text;
          frag.appendChild(span);
        },
        () => frag.appendChild(document.createTextNode("\n")),
      );
      el.replaceChildren(frag);
    })
    .catch(() => {}); // Unknown/failed language chunk: keep the plain text.
}

/** Display name for the badge, e.g. "py" → "Python". */
export function codeLanguageLabel(lang: string): string {
  return LanguageDescription.matchLanguageName(languages, lang, true)?.name ?? lang;
}
