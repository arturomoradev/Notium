import { EditorView } from "@codemirror/view";
import { icons } from "../../../lib/icons";
import { pushBackHandler } from "../../../lib/backStack";
import { editBlockEffect } from "./caches";

const svgIcon = (name: string) =>
  `<svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${icons[name]}</svg>`;

export function iconButton(icon: string, title: string, onClick: () => void): HTMLButtonElement {
  const b = document.createElement("button");
  b.type = "button";
  b.className = "cm-block-action";
  b.title = title;
  b.setAttribute("aria-label", title);
  b.innerHTML = svgIcon(icon);
  // Keep the editor caret where it is.
  b.addEventListener("mousedown", (e) => {
    e.preventDefault();
    e.stopPropagation();
  });
  b.addEventListener("click", (e) => {
    e.preventDefault();
    e.stopPropagation();
    onClick();
  });
  return b;
}

/** Shows the raw source of the block `[from, to]`; it renders again once the cursor leaves it. */
// The block's position is read from its DOM at click time: widgets ignore `from` in `eq` (so
// typing above them doesn't rebuild them), which leaves the instance's own `from` stale.
export function openBlockSource(view: EditorView, dom: HTMLElement, length: number): void {
  const from = view.posAtDOM(dom);
  const to = from + length;
  view.dispatch({ selection: { anchor: from }, effects: editBlockEffect.of({ from, to }) });
  view.focus();
}

/** Edit button (bottom-right), for blocks that stay rendered while the cursor passes over them. */
export function addEditButton(el: HTMLElement, onEdit: () => void): void {
  const bottom = document.createElement("div");
  bottom.className = "cm-block-actions cm-block-actions-bottom";
  bottom.append(iconButton("pencil", "Editar código", onEdit));
  el.append(bottom);
}

/** Copy/export (top-right), click-to-zoom and edit (bottom-right) for a rendered diagram or formula. */
export function addViewerActions(
  el: HTMLElement,
  opts: { code: string; name: string; image: () => Element | null; onEdit?: () => void },
): void {
  el.classList.add("cm-viewable");
  const top = document.createElement("div");
  top.className = "cm-block-actions cm-block-actions-top";
  const copy = iconButton("copy", "Copiar código", () => {
    void navigator.clipboard
      ?.writeText(opts.code)
      .then(() => {
        copy.innerHTML = svgIcon("check");
        setTimeout(() => (copy.innerHTML = svgIcon("copy")), 1200);
      })
      .catch(() => {});
  });
  const exp = iconButton("download", "Exportar como imagen", () => {
    const img = opts.image();
    if (img) void exportImage(img, opts.name).catch((e) => console.error("Export failed:", e));
  });
  top.append(copy, exp);
  el.append(top);
  if (opts.onEdit) addEditButton(el, opts.onEdit);
  el.addEventListener("mousedown", (e) => e.preventDefault());
  el.addEventListener("click", () => {
    const img = opts.image();
    if (img) openLightbox(img);
  });
}

type SavePicker = (opts: unknown) => Promise<{
  createWritable: () => Promise<{ write: (d: Blob) => Promise<void>; close: () => Promise<void> }>;
}>;

/**
 * Android: system "save as" via the native bridge; PC (WebView2): file picker, opened before
 * rendering because it needs the click's user activation; plain browsers: a download.
 */
async function saveBlob(filename: string, render: () => Promise<Blob>): Promise<void> {
  const w = window as unknown as {
    NotiumAndroid?: { saveImageDocument?: (name: string, base64: string) => void };
    showSaveFilePicker?: SavePicker;
  };
  if (w.NotiumAndroid?.saveImageDocument) {
    const blob = await render();
    const base64 = await new Promise<string>((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => resolve((reader.result as string).split(",")[1] ?? "");
      reader.onerror = () => reject(reader.error);
      reader.readAsDataURL(blob);
    });
    w.NotiumAndroid.saveImageDocument(filename, base64);
    return;
  }
  if (w.showSaveFilePicker) {
    let handle: Awaited<ReturnType<SavePicker>>;
    try {
      handle = await w.showSaveFilePicker({
        suggestedName: filename,
        types: [{ description: "Imagen PNG", accept: { "image/png": [".png"] } }],
      });
    } catch (e) {
      if ((e as Error).name === "AbortError") return;
      throw e;
    }
    const blob = await render();
    const out = await handle.createWritable();
    await out.write(blob);
    await out.close();
    return;
  }
  const blob = await render();
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  a.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

const attrSafe = (s: string) => s.replace(/"/g, "'");

async function dataUrl(url: string): Promise<string> {
  const blob = await (await fetch(url)).blob();
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(reader.result as string);
    reader.onerror = () => reject(reader.error);
    reader.readAsDataURL(blob);
  });
}

/**
 * KaTeX's styles with its fonts inlined: an SVG drawn as an image can't load external files,
 * so without this an exported formula falls back to system fonts.
 */
async function mathCss(): Promise<string> {
  const out: Promise<string>[] = [];
  for (const sheet of Array.from(document.styleSheets)) {
    let rules: CSSRuleList;
    try {
      rules = sheet.cssRules;
    } catch {
      continue; // cross-origin sheet
    }
    for (const r of Array.from(rules)) {
      if (r instanceof CSSFontFaceRule) {
        const family = r.style.getPropertyValue("font-family");
        const src = /url\(["']?([^"')]+\.woff2)["']?\)/.exec(r.cssText)?.[1];
        if (!family.includes("KaTeX") || !src) continue;
        const style = r.style.getPropertyValue("font-style") || "normal";
        const weight = r.style.getPropertyValue("font-weight") || "normal";
        out.push(
          dataUrl(new URL(src, sheet.href ?? location.href).href).then(
            (data) =>
              `@font-face{font-family:${family};font-style:${style};font-weight:${weight};src:url(${data}) format("woff2")}`,
            () => "",
          ),
        );
      } else if (/katex|cm-math/.test(r.cssText)) {
        out.push(Promise.resolve(r.cssText));
      }
    }
  }
  return (await Promise.all(out)).join("\n").replace(/&/g, "&amp;").replace(/</g, "&lt;");
}

/** Saves an SVG diagram or an HTML formula as a PNG. */
export function exportImage(source: Element, name: string): Promise<void> {
  return saveBlob(`${name}.png`, () => renderPng(source));
}

async function renderPng(source: Element): Promise<Blob> {
  const cs = getComputedStyle(source);
  const bodyFont = attrSafe(getComputedStyle(document.body).fontFamily);
  const rect = source.getBoundingClientRect();
  let w: number;
  let h: number;
  let svgText: string;
  if (source instanceof SVGSVGElement) {
    const vb = source.viewBox.baseVal;
    w = Math.ceil(vb?.width || rect.width);
    h = Math.ceil(vb?.height || rect.height);
    const clone = source.cloneNode(true) as SVGSVGElement;
    clone.setAttribute("width", String(w));
    clone.setAttribute("height", String(h));
    clone.style.maxWidth = "none";
    // Mermaid is set to font-family "inherit", which means nothing outside the page.
    svgText = new XMLSerializer()
      .serializeToString(clone)
      .replace(/font-family:\s*inherit/g, `font-family:${bodyFont}`);
  } else {
    const pad = 24;
    w = Math.ceil(rect.width) + pad * 2;
    h = Math.ceil(rect.height) + pad * 2;
    const html = new XMLSerializer().serializeToString(source.cloneNode(true));
    svgText =
      `<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}"><foreignObject width="100%" height="100%">` +
      `<div xmlns="http://www.w3.org/1999/xhtml" style="display:flex;align-items:center;justify-content:center;width:${w}px;height:${h}px;font-family:${attrSafe(cs.fontFamily)};font-size:${cs.fontSize};color:${cs.color}">` +
      `<style>${await mathCss()}</style>${html}</div></foreignObject></svg>`;
  }
  const img = new Image();
  img.src = "data:image/svg+xml;charset=utf-8," + encodeURIComponent(svgText);
  await img.decode();
  const scale = 2;
  const canvas = document.createElement("canvas");
  canvas.width = w * scale;
  canvas.height = h * scale;
  const ctx = canvas.getContext("2d")!;
  ctx.fillStyle = getComputedStyle(document.body).backgroundColor;
  ctx.fillRect(0, 0, canvas.width, canvas.height);
  ctx.drawImage(img, 0, 0, canvas.width, canvas.height);
  const png = await new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, "image/png"));
  if (!png) throw new Error("No se pudo generar la imagen");
  return png;
}

/** Full-screen view of a diagram or formula with wheel/pinch zoom and drag to pan. */
export function openLightbox(source: Element): void {
  const overlay = document.createElement("div");
  overlay.className = "cm-lightbox";
  overlay.setAttribute("role", "dialog");
  overlay.setAttribute("aria-modal", "true");
  const stage = document.createElement("div");
  stage.className = "cm-lightbox-stage";
  const clone = source.cloneNode(true) as Element;
  if (clone instanceof SVGSVGElement) clone.removeAttribute("style");
  stage.append(clone);

  const bar = document.createElement("div");
  bar.className = "cm-lightbox-bar";
  const label = document.createElement("span");
  label.className = "cm-lightbox-zoom";

  let scale = 1;
  let x = 0;
  let y = 0;
  const apply = () => {
    stage.style.transform = `translate(${x}px, ${y}px) scale(${scale})`;
    label.textContent = `${Math.round(scale * 100)}%`;
  };
  // Zoom keeping the point (cx, cy) under the pointer still; the stage is centred in the overlay.
  const zoomBy = (factor: number, cx = innerWidth / 2, cy = innerHeight / 2) => {
    const next = Math.min(12, Math.max(0.2, scale * factor));
    const k = next / scale;
    const px = cx - innerWidth / 2;
    const py = cy - innerHeight / 2;
    x = px - (px - x) * k;
    y = py - (py - y) * k;
    scale = next;
    apply();
  };
  const reset = () => {
    scale = 1;
    x = 0;
    y = 0;
    apply();
  };

  bar.append(
    iconButton("minus", "Alejar", () => zoomBy(1 / 1.25)),
    label,
    iconButton("plus", "Acercar", () => zoomBy(1.25)),
    iconButton("restore", "Restablecer zoom", reset),
    iconButton("x", "Cerrar", () => close()),
  );
  overlay.append(stage, bar);

  const pointers = new Map<number, { x: number; y: number }>();
  let moved = false;
  let downOnBackdrop = false;
  overlay.addEventListener("pointerdown", (e) => {
    if ((e.target as Element).closest(".cm-lightbox-bar")) return;
    overlay.setPointerCapture(e.pointerId);
    pointers.set(e.pointerId, { x: e.clientX, y: e.clientY });
    if (pointers.size === 1) {
      moved = false;
      downOnBackdrop = !stage.contains(e.target as Node);
    }
  });
  overlay.addEventListener("pointermove", (e) => {
    const prev = pointers.get(e.pointerId);
    if (!prev) return;
    if (pointers.size === 1) {
      x += e.clientX - prev.x;
      y += e.clientY - prev.y;
      if (Math.abs(e.clientX - prev.x) + Math.abs(e.clientY - prev.y) > 1) moved = true;
      pointers.set(e.pointerId, { x: e.clientX, y: e.clientY });
      apply();
      return;
    }
    const [a, b] = [...pointers.values()];
    const other = a === prev ? b : a;
    const before = Math.hypot(prev.x - other.x, prev.y - other.y);
    const after = Math.hypot(e.clientX - other.x, e.clientY - other.y);
    pointers.set(e.pointerId, { x: e.clientX, y: e.clientY });
    moved = true;
    if (before > 0) zoomBy(after / before, (e.clientX + other.x) / 2, (e.clientY + other.y) / 2);
  });
  const release = (e: PointerEvent) => {
    if (!pointers.delete(e.pointerId)) return;
    // A plain click on the backdrop (not on the content) closes.
    if (pointers.size === 0 && !moved && downOnBackdrop) close();
  };
  overlay.addEventListener("pointerup", release);
  overlay.addEventListener("pointercancel", (e) => pointers.delete(e.pointerId));
  overlay.addEventListener(
    "wheel",
    (e) => {
      e.preventDefault();
      zoomBy(e.deltaY < 0 ? 1.15 : 1 / 1.15, e.clientX, e.clientY);
    },
    { passive: false },
  );
  overlay.addEventListener("dblclick", (e) => {
    if (scale === 1) zoomBy(2, e.clientX, e.clientY);
    else reset();
  });

  const onKey = (e: KeyboardEvent) => {
    if (e.key === "Escape") close();
    else if (e.key === "+" || e.key === "=") zoomBy(1.25);
    else if (e.key === "-") zoomBy(1 / 1.25);
    else if (e.key === "0") reset();
    else return;
    e.preventDefault();
    e.stopPropagation();
  };
  const popBack = pushBackHandler(() => {
    close();
    return true;
  });
  function close() {
    overlay.remove();
    window.removeEventListener("keydown", onKey, true);
    popBack();
  }
  window.addEventListener("keydown", onKey, true);
  document.body.append(overlay);
  apply();
}
