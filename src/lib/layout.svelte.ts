/**
 * Single source of truth for the responsive layout.
 *
 * - `mobilePlatform`: the app runs on a phone/tablet OS (touch-first; decides
 *   platform features such as the camera or the native folder picker).
 * - `compact`: use the phone-style UI (overlay drawers, large touch targets,
 *   bottom-sheet settings). True on mobile platforms at any size and on
 *   desktop windows narrower than `COMPACT_MAX_WIDTH`.
 *
 * The `is-compact` class on `<html>` mirrors `compact`, and all compact CSS
 * hangs off it, so markup and styles can never disagree.
 */
export const COMPACT_MAX_WIDTH = 767;

function detectMobilePlatform(): boolean {
  if (typeof window === "undefined") return false;
  // Dev-only override so the phone/tablet UI can be previewed in a desktop browser.
  if (import.meta.env.DEV && new URLSearchParams(window.location.search).has("mobile")) return true;
  return /Android|iPhone|iPad|iPod/i.test(navigator.userAgent);
}

class LayoutStore {
  readonly mobilePlatform = detectMobilePlatform();
  narrow = $state(false);

  constructor() {
    if (typeof window === "undefined") return;
    const query = window.matchMedia(`(max-width: ${COMPACT_MAX_WIDTH}px)`);
    const apply = (): void => {
      this.narrow = query.matches;
      document.documentElement.classList.toggle("is-compact", this.compact);
    };
    query.addEventListener("change", apply);
    apply();
  }

  get compact(): boolean {
    return this.mobilePlatform || this.narrow;
  }
}

export const layout = new LayoutStore();
