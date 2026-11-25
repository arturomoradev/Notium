/**
 * Keeps the Android window background (visible behind the transparent status
 * and navigation bars) in sync with the surface currently touching the screen
 * edges: the editor (`--bg`) or an open drawer (`--panel`). No-op elsewhere.
 */
type AndroidBridge = { setSystemBarColor?: (hex: string) => void };

let lastColor = "";

export function syncSystemBars(surface: "editor" | "panel"): void {
  if (typeof window === "undefined") return;
  const bridge = (window as unknown as { NotiumAndroid?: AndroidBridge }).NotiumAndroid;
  if (!bridge?.setSystemBarColor) return;
  const token = surface === "panel" ? "--panel" : "--bg";
  let color = getComputedStyle(document.documentElement).getPropertyValue(token).trim();
  // The CSS minifier shortens `#000000` to `#000`; the bridge wants `#rrggbb`.
  if (/^#[0-9a-fA-F]{3}$/.test(color)) color = "#" + [...color.slice(1)].map((c) => c + c).join("");
  if (!/^#[0-9a-fA-F]{6}$/.test(color) || color === lastColor) return;
  lastColor = color;
  bridge.setSystemBarColor(color);
}
