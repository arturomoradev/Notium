import { EditorView, WidgetType } from "@codemirror/view";
import { call, readAttachment } from "../../../lib/ipc/commands";
import { mdToPlainText } from "../../../lib/plainText";
import { blockGap } from "./blockWidgets";
import {
  attachmentUrlCache,
  imageMime,
  EMBED_CACHE_TTL_MS,
  embedExcerptCache,
  extractHeadingSection,
  normalizeHeading,
} from "./caches";

export class NoteEmbedWidget extends WidgetType {
  constructor(
    private readonly rawTarget: string,
    private readonly onOpen: (target: string) => void,
  ) {
    super();
  }
  eq(other: NoteEmbedWidget): boolean {
    return other.rawTarget === this.rawTarget;
  }
  toDOM(view: EditorView): HTMLElement {
    const clean = this.rawTarget.split("|")[0].trim();
    const [noteRef, headingRef] = clean.split("#");
    const card = document.createElement("div");
    card.className = "cm-note-embed";

    const header = document.createElement("div");
    header.className = "cm-note-embed-header";
    const titleEl = document.createElement("span");
    titleEl.className = "cm-note-embed-title";
    titleEl.textContent = headingRef
      ? `📄 ${noteRef.trim()} › #${headingRef.trim()}`
      : `📄 ${noteRef.trim()}`;
    const openBtn = document.createElement("button");
    openBtn.type = "button";
    openBtn.className = "cm-note-embed-open";
    openBtn.textContent = "Abrir ↗";
    openBtn.addEventListener("mousedown", (e) => {
      e.preventDefault();
      e.stopPropagation();
      this.onOpen(clean);
    });
    header.append(titleEl, openBtn);
    card.appendChild(header);

    const bodyEl = document.createElement("div");
    bodyEl.className = "cm-note-embed-body";
    card.appendChild(bodyEl);

    const cached = embedExcerptCache.get(clean);
    if (cached && Date.now() - cached.ts < EMBED_CACHE_TTL_MS) {
      bodyEl.textContent = cached.text;
      if (cached.muted) bodyEl.classList.add("muted");
      return blockGap(card);
    }

    bodyEl.textContent = "Cargando nota incrustada…";

    void call("note_list")
      .then(async (list) => {
        const refNorm = normalizeHeading(noteRef);
        const found = list.find(
          (n) =>
            n.note_id === noteRef.trim() ||
            normalizeHeading(n.title) === refNorm ||
            normalizeHeading(n.path) === refNorm,
        );
        if (!found) {
          const text = `Nota «${noteRef.trim()}» no encontrada`;
          bodyEl.textContent = text;
          bodyEl.classList.add("muted");
          embedExcerptCache.set(clean, { text, muted: true, ts: Date.now() });
          view.requestMeasure();
          return;
        }
        const full = await call("note_read", { noteId: found.note_id });
        const rawExcerpt = headingRef
          ? extractHeadingSection(full.content, headingRef)
          : full.content;
        const plain = mdToPlainText(rawExcerpt).trim();
        const text = plain
          ? plain.length > 420
            ? plain.slice(0, 420) + "…"
            : plain
          : "(Nota vacía)";
        bodyEl.textContent = text;
        embedExcerptCache.set(clean, { text, muted: false, ts: Date.now() });
        view.requestMeasure();
      })
      .catch(() => {
        bodyEl.textContent = "No se pudo cargar la vista previa.";
        view.requestMeasure();
      });

    return blockGap(card);
  }
  ignoreEvent(): boolean {
    return false;
  }
}

export class AttachmentWidget extends WidgetType {
  constructor(private readonly path: string) {
    super();
  }
  eq(other: AttachmentWidget): boolean {
    return other.path === this.path;
  }
  toDOM(view: EditorView): HTMLElement {
    const imageFile = /\.(png|jpe?g|gif|webp|svg)$/i.test(this.path);
    if (imageFile) {
      const image = document.createElement("img");
      image.className = "cm-vault-image";
      image.alt = this.path.split("/").pop() ?? "Adjunto";
      image.decoding = "async";
      image.addEventListener("load", () => view.requestMeasure());
      let url = attachmentUrlCache.get(this.path);
      if (!url) {
        const path = this.path;
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
    const link = document.createElement("a");
    link.className = "cm-attachment-link";
    link.textContent = `📎 ${this.path.split("/").pop() ?? "Adjunto"}`;
    link.href = "#";
    link.addEventListener("click", (event) => {
      event.preventDefault();
      void readAttachment(this.path)
        .then((bytes) => {
          const blob = new Blob([bytes]);
          const url = URL.createObjectURL(blob);
          const download = document.createElement("a");
          download.href = url;
          download.download = this.path.split("/").pop() ?? "attachment";
          download.click();
          URL.revokeObjectURL(url);
        })
        .catch(() => {});
    });
    return link;
  }
}

export class ExternalImageWidget extends WidgetType {
  constructor(
    private readonly alt: string,
    private readonly src: string,
  ) {
    super();
  }
  eq(other: ExternalImageWidget): boolean {
    return other.alt === this.alt && other.src === this.src;
  }
  toDOM(view: EditorView): HTMLElement {
    const img = document.createElement("img");
    img.alt = this.alt;
    img.src = this.src;
    img.className = "cm-vault-image";
    img.decoding = "async";
    img.addEventListener("load", () => view.requestMeasure());
    return img;
  }
}

export class MarkdownLinkWidget extends WidgetType {
  constructor(
    private readonly label: string,
    private readonly href: string,
  ) {
    super();
  }
  eq(other: MarkdownLinkWidget): boolean {
    return other.label === this.label && other.href === this.href;
  }
  toDOM(): HTMLElement {
    const anchor = document.createElement("a");
    anchor.className = "cm-rendered-link";
    anchor.textContent = this.label;
    anchor.href = this.href;
    anchor.target = "_blank";
    anchor.rel = "noopener noreferrer";
    anchor.addEventListener("mousedown", (event) => {
      event.preventDefault();
      event.stopPropagation();
      window.open(this.href, "_blank", "noopener,noreferrer");
    });
    return anchor;
  }
  ignoreEvent(): boolean {
    return false;
  }
}

export class WikiLinkWidget extends WidgetType {
  constructor(
    private readonly rawTarget: string,
    private readonly onOpen: (target: string) => void,
  ) {
    super();
  }
  eq(other: WikiLinkWidget): boolean {
    return other.rawTarget === this.rawTarget;
  }
  toDOM(): HTMLElement {
    const [targetPart, aliasPart] = this.rawTarget.split("|");
    const cleanTarget = (targetPart ?? "").trim();
    const alias = aliasPart?.trim();
    let label = alias || cleanTarget;
    if (!alias) {
      const hashIdx = cleanTarget.indexOf("#");
      if (hashIdx === 0) {
        label = `§ ${cleanTarget.slice(1).trim()}`;
      } else if (hashIdx > 0) {
        const noteName = cleanTarget.slice(0, hashIdx).trim();
        const secName = cleanTarget.slice(hashIdx + 1).trim();
        label = secName ? `${noteName} › § ${secName}` : noteName;
      }
    }
    const span = document.createElement("span");
    span.className = "cm-wiki-link cm-wiki-link-pill";
    span.textContent = label;
    span.title = `Ir a ${cleanTarget}`;
    const activate = (e: Event) => {
      e.preventDefault();
      e.stopPropagation();
      this.onOpen(this.rawTarget.trim());
    };
    span.addEventListener("mousedown", activate);
    span.addEventListener("click", activate);
    return span;
  }
  ignoreEvent(): boolean {
    return false;
  }
}
