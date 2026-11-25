// Mobile keyboard/viewport helpers (carried over from the Phase 0A spike).

/** Track the visual viewport so CSS layout uses `--app-height` above the IME. */
export function updateViewport(): void {
  const vv = window.visualViewport;
  const height = vv?.height ?? window.innerHeight;
  const offsetTop = vv?.offsetTop ?? 0;
  document.documentElement.style.setProperty("--app-height", `${Math.round(height)}px`);
  document.documentElement.style.setProperty("--vv-offset-top", `${Math.round(offsetTop)}px`);
}

let revealTimer: number | undefined;
let revealRaf = 0;

/**
 * Reveal the caret after the IME/keyboard animation settles: clear pending work,
 * then run `reveal` on a double requestAnimationFrame. `delayMs` lets the caller
 * wait out the keyboard open/close animation (0A used 320 ms).
 */
export function scheduleCaretReveal(reveal: () => void, delayMs = 0): void {
  if (revealTimer !== undefined) window.clearTimeout(revealTimer);
  if (revealRaf) cancelAnimationFrame(revealRaf);
  const run = (): void => {
    revealTimer = undefined;
    revealRaf = requestAnimationFrame(() => {
      revealRaf = requestAnimationFrame(() => {
        revealRaf = 0;
        reveal();
      });
    });
  };
  if (delayMs > 0) revealTimer = window.setTimeout(run, delayMs);
  else run();
}
