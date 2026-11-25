import { EditorView, WidgetType } from "@codemirror/view";

/** An interactive `- [ ]` / `- [x]` checkbox widget. */
export class CheckboxWidget extends WidgetType {
  constructor(
    private readonly checkPos: number,
    private readonly checked: boolean,
  ) {
    super();
  }
  eq(other: CheckboxWidget): boolean {
    return other.checkPos === this.checkPos && other.checked === this.checked;
  }
  toDOM(view: EditorView): HTMLElement {
    const box = document.createElement("input");
    box.type = "checkbox";
    box.checked = this.checked;
    box.className = "cm-task-checkbox";
    box.addEventListener("mousedown", (e) => {
      e.preventDefault();
      e.stopPropagation();
    });
    box.addEventListener("change", () => {
      view.dispatch({
        changes: {
          from: this.checkPos,
          to: this.checkPos + 1,
          insert: box.checked ? "x" : " ",
        },
      });
    });
    return box;
  }
  ignoreEvent(): boolean {
    return false;
  }
}

export class BulletWidget extends WidgetType {
  toDOM(): HTMLElement {
    const dot = document.createElement("span");
    dot.className = "cm-bullet-marker";
    dot.textContent = "•";
    return dot;
  }
  eq(): boolean {
    return true;
  }
}

export class NumberWidget extends WidgetType {
  constructor(private readonly marker: string) {
    super();
  }
  eq(other: NumberWidget): boolean {
    return other.marker === this.marker;
  }
  toDOM(): HTMLElement {
    const marker = document.createElement("span");
    marker.className = "cm-number-marker";
    marker.textContent = this.marker;
    return marker;
  }
}
