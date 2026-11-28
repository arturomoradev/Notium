/**
 * Per-device editor colours (headings, bold, italic, background…) for each theme, plus an
 * optional user CSS file. Applied as CSS variables that app.css reads with its own defaults
 * as fallback, so an unset colour keeps the theme's look.
 */
export const EDITOR_COLOR_KEYS = [
  { key: "bg", label: "Fondo (editor y app)" },
  { key: "text", label: "Texto (editor y app)" },
  { key: "h1", label: "Heading 1" },
  { key: "h2", label: "Heading 2" },
  { key: "h3", label: "Heading 3" },
  { key: "h4", label: "Heading 4" },
  { key: "h5", label: "Heading 5" },
  { key: "h6", label: "Heading 6" },
  { key: "bold", label: "Negrita" },
  { key: "italic", label: "Cursiva" },
  { key: "strike", label: "Tachado" },
  { key: "highlight", label: "Fondo del resaltado ==texto==" },
  { key: "code", label: "Código en línea" },
  { key: "link", label: "Enlaces y color de acento" },
  { key: "tag", label: "Etiquetas #tag" },
  { key: "quote", label: "Borde de las citas" },
  { key: "marker", label: "Viñetas y números de lista" },
] as const;

export type EditorColorKey = (typeof EDITOR_COLOR_KEYS)[number]["key"];
export type ThemeName = "dark" | "light";
type Colors = Partial<Record<EditorColorKey, string>>;
type CustomCss = { name: string; css: string } | null;
export type SavedTheme = {
  name: string;
  colors: Record<ThemeName, Colors>;
  css: CustomCss;
  builtIn?: boolean;
};

/** Ready-made themes; they can be applied but not deleted. */
export const BUILT_IN_THEMES: SavedTheme[] = [
  // No colours and no CSS: the app's own palette (applying it is the same as "Restablecer").
  { name: "Notium original", builtIn: true, css: null, colors: { dark: {}, light: {} } },
  {
    name: "One Dark",
    builtIn: true,
    css: null,
    // Atom's One Dark for the dark theme and its sibling One Light for the light one.
    colors: {
      dark: {
        bg: "#282c34",
        text: "#abb2bf",
        h1: "#e06c75",
        h2: "#d19a66",
        h3: "#e5c07b",
        h4: "#98c379",
        h5: "#61afef",
        h6: "#c678dd",
        bold: "#e5c07b",
        italic: "#c678dd",
        strike: "#5c6370",
        highlight: "#3e4451",
        code: "#98c379",
        link: "#61afef",
        tag: "#56b6c2",
        quote: "#5c6370",
        marker: "#5c6370",
      },
      light: {
        bg: "#fafafa",
        text: "#383a42",
        h1: "#e45649",
        h2: "#986801",
        h3: "#c18401",
        h4: "#50a14f",
        h5: "#4078f2",
        h6: "#a626a4",
        bold: "#c18401",
        italic: "#a626a4",
        strike: "#a0a1a7",
        highlight: "#e5e5e6",
        code: "#50a14f",
        link: "#4078f2",
        tag: "#0184bc",
        quote: "#a0a1a7",
        marker: "#a0a1a7",
      },
    },
  },
  {
    name: "Dracula",
    builtIn: true,
    css: null,
    // The official Dracula palette, with its light sibling Alucard for the light theme.
    colors: {
      dark: {
        bg: "#282a36",
        text: "#f8f8f2",
        h1: "#ff79c6",
        h2: "#bd93f9",
        h3: "#8be9fd",
        h4: "#50fa7b",
        h5: "#ffb86c",
        h6: "#f1fa8c",
        bold: "#ffb86c",
        italic: "#f1fa8c",
        strike: "#6272a4",
        highlight: "#44475a",
        code: "#50fa7b",
        link: "#bd93f9",
        tag: "#8be9fd",
        quote: "#6272a4",
        marker: "#6272a4",
      },
      light: {
        bg: "#fffbeb",
        text: "#1f1f1f",
        h1: "#a3144d",
        h2: "#644ac9",
        h3: "#036a96",
        h4: "#14710a",
        h5: "#a34d14",
        h6: "#846e15",
        bold: "#a34d14",
        italic: "#846e15",
        strike: "#635d97",
        highlight: "#cfcfde",
        code: "#14710a",
        link: "#644ac9",
        tag: "#036a96",
        quote: "#635d97",
        marker: "#635d97",
      },
    },
  },
];

const COLORS_KEY = "notium.editorColors";
const CSS_KEY = "notium.customCss";
const THEMES_KEY = "notium.editorThemes";
const HEX = /^#[0-9a-f]{6}$/i;
const KNOWN = new Set<string>(EDITOR_COLOR_KEYS.map((k) => k.key));

/** Keeps only known keys with `#rrggbb` values: imported files and storage are untrusted. */
function clean(input: unknown): Colors {
  const out: Colors = {};
  if (input && typeof input === "object") {
    for (const [k, v] of Object.entries(input)) {
      if (KNOWN.has(k) && typeof v === "string" && HEX.test(v)) out[k as EditorColorKey] = v;
    }
  }
  return out;
}

function load<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(key);
    return raw === null ? fallback : (JSON.parse(raw) as T);
  } catch {
    return fallback;
  }
}

class EditorColorsStore {
  colors = $state<Record<ThemeName, Colors>>({ dark: {}, light: {} });
  customCss = $state<CustomCss>(null);
  /** The user's saved themes (built-in ones are in BUILT_IN_THEMES). */
  themes = $state<SavedTheme[]>([]);

  constructor() {
    if (typeof window === "undefined") return;
    const saved = load<Record<string, unknown>>(COLORS_KEY, {});
    this.colors = { dark: clean(saved.dark), light: clean(saved.light) };
    this.customCss = load(CSS_KEY, null);
    const themes = load<unknown>(THEMES_KEY, []);
    this.themes = (Array.isArray(themes) ? themes : [])
      .filter((t) => t && typeof t.name === "string" && t.name.trim())
      .map((t) => ({
        name: String(t.name).trim(),
        colors: { dark: clean(t.colors?.dark), light: clean(t.colors?.light) },
        css:
          t.css && typeof t.css.css === "string"
            ? { name: String(t.css.name), css: t.css.css }
            : null,
      }));
    this.apply();
  }

  /** Name of the saved or built-in theme matching what's applied now, if any. */
  get activeTheme(): string | null {
    const now = JSON.stringify([this.colors, this.customCss]);
    return (
      [...BUILT_IN_THEMES, ...this.themes].find((t) => JSON.stringify([t.colors, t.css]) === now)
        ?.name ?? null
    );
  }

  /** Saves the current colours and CSS under `name`, replacing a saved theme of that name. */
  saveTheme(name: string): string {
    const n = name.trim();
    if (!n) return "Escribe un nombre para el tema.";
    if (BUILT_IN_THEMES.some((t) => t.name.toLowerCase() === n.toLowerCase())) {
      return `«${n}» es un tema incluido; usa otro nombre.`;
    }
    const theme: SavedTheme = {
      name: n,
      colors: $state.snapshot(this.colors),
      css: $state.snapshot(this.customCss),
    };
    const replaced = this.themes.some((t) => t.name.toLowerCase() === n.toLowerCase());
    this.themes = [...this.themes.filter((t) => t.name.toLowerCase() !== n.toLowerCase()), theme];
    this.persist();
    return replaced ? `Tema «${n}» actualizado.` : `Tema «${n}» guardado.`;
  }

  applySaved(theme: SavedTheme): void {
    this.colors = $state.snapshot(theme.colors);
    this.customCss = theme.css ? { ...theme.css } : null;
    this.persist();
  }

  deleteTheme(name: string): void {
    this.themes = this.themes.filter((t) => t.name !== name);
    this.persist();
  }

  set(theme: ThemeName, key: EditorColorKey, value: string | null): void {
    const next = { ...this.colors[theme] };
    if (value && HEX.test(value)) next[key] = value;
    else delete next[key];
    this.colors = { ...this.colors, [theme]: next };
    this.persist();
  }

  clearCustomCss(): void {
    this.customCss = null;
    this.persist();
  }

  resetAll(): void {
    this.colors = { dark: {}, light: {} };
    this.customCss = null;
    this.persist();
  }

  /**
   * `.json`: `{ "h1": "#ff0000", … }` for the current theme, or `{ "dark": {…}, "light": {…} }`.
   * `.css`: any stylesheet, kept as the custom CSS. Returns a message for the user.
   */
  importFile(name: string, text: string, theme: ThemeName): string {
    if (/\.css$/i.test(name)) {
      this.customCss = { name, css: text };
      this.persist();
      return `CSS «${name}» aplicado.`;
    }
    let data: unknown;
    try {
      data = JSON.parse(text);
    } catch {
      return "El archivo no es un JSON válido.";
    }
    const obj = (data ?? {}) as Record<string, unknown>;
    const perTheme = "dark" in obj || "light" in obj;
    const next = perTheme
      ? {
          dark: { ...this.colors.dark, ...clean(obj.dark) },
          light: { ...this.colors.light, ...clean(obj.light) },
        }
      : { ...this.colors, [theme]: { ...this.colors[theme], ...clean(obj) } };
    const count = Object.keys(
      perTheme ? { ...clean(obj.dark), ...clean(obj.light) } : clean(obj),
    ).length;
    if (count === 0) return "El JSON no tiene colores reconocidos (#rrggbb).";
    this.colors = next;
    this.persist();
    return `${count} colores importados.`;
  }

  private persist(): void {
    try {
      localStorage.setItem(COLORS_KEY, JSON.stringify(this.colors));
      localStorage.setItem(THEMES_KEY, JSON.stringify(this.themes));
      if (this.customCss) localStorage.setItem(CSS_KEY, JSON.stringify(this.customCss));
      else localStorage.removeItem(CSS_KEY);
    } catch {
      // Storage full or unavailable: the colours still apply for this session.
    }
    this.apply();
  }

  /** One <style> per theme block, so switching theme needs no re-apply. */
  apply(): void {
    const vars = (c: Colors) =>
      Object.entries(c)
        .map(([k, v]) => `--ed-${k}:${v};`)
        .join("");
    styleEl("notium-editor-colors").textContent =
      `:root[data-theme="dark"]{${vars(this.colors.dark)}${appVars(this.colors.dark, "dark")}}` +
      `:root[data-theme="light"]{${vars(this.colors.light)}${appVars(this.colors.light, "light")}}`;
    styleEl("notium-custom-css").textContent = this.customCss?.css ?? "";
  }
}

/**
 * The whole app follows the theme too: background, text and link colours override app.css's
 * palette, and panels, borders and muted text are derived from them so the chrome matches.
 */
function appVars(c: Colors, theme: ThemeName): string {
  // Panels step towards white on dark themes and towards black on light ones.
  const step = theme === "dark" ? "white" : "black";
  const out: string[] = [];
  if (c.bg) {
    out.push(
      `--bg:${c.bg}`,
      `--panel:color-mix(in srgb,${c.bg} 94%,${step})`,
      `--panel-2:color-mix(in srgb,${c.bg} 86%,${step})`,
      `--pill-bg:color-mix(in srgb,${c.bg} 84%,${step})`,
      `--pill-border:color-mix(in srgb,${c.bg} 78%,${step})`,
      `--border:color-mix(in srgb,${c.bg} 82%,${step})`,
      `--code-bg:color-mix(in srgb,${c.bg} 92%,black)`,
    );
  }
  if (c.text) {
    out.push(
      `--text:${c.text}`,
      `--active-text:${c.text}`,
      `--code-fg:${c.text}`,
      `--muted:color-mix(in srgb,${c.text} 72%,var(--bg))`,
      `--faint:color-mix(in srgb,${c.text} 42%,var(--bg))`,
    );
  }
  if (c.link) {
    out.push(
      `--accent:${c.link}`,
      `--accent-text:${c.link}`,
      `--accent-hover:color-mix(in srgb,${c.link} 85%,${step})`,
      `--caret:${c.link}`,
      `--selection-bg:color-mix(in srgb,${c.link} 33%,transparent)`,
      `--focus-ring:color-mix(in srgb,${c.link} 50%,transparent)`,
    );
  }
  return out.map((d) => d + ";").join("");
}

function styleEl(id: string): HTMLStyleElement {
  let el = document.getElementById(id) as HTMLStyleElement | null;
  if (!el) {
    el = document.createElement("style");
    el.id = id;
    // After app.css, so user rules win at equal specificity.
    document.head.append(el);
  }
  return el;
}

export const editorColorsStore = new EditorColorsStore();
