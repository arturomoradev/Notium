/**
 * Svelte action: when the text of `node` doesn't fit, its `.marquee-inner` child scrolls from
 * right to left to show the end, pauses, and starts over (CSS in app.css, off with reduced
 * motion). `node` must clip its content (overflow: hidden; white-space: nowrap).
 */
export function marquee(node: HTMLElement): { destroy(): void } {
  const update = () => {
    const overflow = node.scrollWidth - node.clientWidth;
    const on = overflow > 1;
    node.classList.toggle("marquee", on);
    if (on) {
      node.style.setProperty("--marquee-dist", `-${overflow}px`);
      // About 30 px per second for the scroll part, never too fast for short overflows.
      node.style.setProperty("--marquee-time", `${Math.max(5, overflow / 30 / 0.6)}s`);
    }
  };
  const resize = new ResizeObserver(update);
  resize.observe(node);
  // Svelte rewrites the text in place when a note is renamed.
  const mutation = new MutationObserver(update);
  mutation.observe(node, { characterData: true, childList: true, subtree: true });
  update();
  return {
    destroy() {
      resize.disconnect();
      mutation.disconnect();
    },
  };
}
