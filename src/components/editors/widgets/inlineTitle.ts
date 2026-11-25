import { StateField, type Extension } from "@codemirror/state";
import { Decoration, EditorView, WidgetType, type DecorationSet } from "@codemirror/view";

/** Renames the note; resolves false when the rename did not happen (the title is restored). */
export type RenameTitle = (title: string) => Promise<boolean>;

class InlineTitleWidget extends WidgetType {
  constructor(
    private readonly title: string,
    private readonly onRename: RenameTitle,
  ) {
    super();
  }
  eq(other: InlineTitleWidget): boolean {
    return other.title === this.title;
  }
  toDOM(view: EditorView): HTMLElement {
    const field = document.createElement("textarea");
    field.className = "cm-inline-title";
    field.rows = 1;
    field.value = this.title;
    field.spellcheck = false;
    field.setAttribute("aria-label", "Título de la nota");
    let current = this.title;

    const commit = async (): Promise<void> => {
      const next = field.value.replace(/\s+/g, " ").trim();
      if (!next || next === current) {
        field.value = current;
        return;
      }
      if (await this.onRename(next)) current = next;
      else field.value = current;
    };

    field.addEventListener("keydown", (e) => {
      if (e.key === "Enter") {
        e.preventDefault();
        // Enter continues into the note body, like Obsidian.
        view.dispatch({ selection: { anchor: 0 } });
        view.focus();
      } else if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        field.value = current;
        field.blur();
      }
    });
    field.addEventListener("blur", () => void commit());
    return field;
  }
  ignoreEvent(): boolean {
    return true;
  }
}

/** Editable note title shown above the first line; it scrolls with the document. */
export function inlineTitle(title: string, onRename: RenameTitle): Extension {
  const deco = Decoration.set([
    Decoration.widget({
      widget: new InlineTitleWidget(title, onRename),
      block: true,
      side: -1,
    }).range(0),
  ]);
  return StateField.define<DecorationSet>({
    create: () => deco,
    update: (d, tr) => d.map(tr.changes),
    provide: (f) => EditorView.decorations.from(f),
  });
}
