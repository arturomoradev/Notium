// Startup splash painted by index.html (and web/index.html) before any script runs.
const MIN_VISIBLE_MS = 600;

/** Fade the splash out once the app is ready; keeps the entry animation from being cut short. */
export function hideSplash(): void {
  const el = document.getElementById("splash");
  if (!el) return;
  setTimeout(
    () => {
      el.classList.add("out");
      setTimeout(() => el.remove(), 400);
    },
    Math.max(0, MIN_VISIBLE_MS - performance.now()),
  );
}
