/**
 * Back-navigation stack for the Android back button / back gesture.
 *
 * Each open layer (modal, panel, note…) registers a handler that returns
 * `true` when it consumed the back action. Handlers run newest-first, so the
 * layer opened last closes first. When none consumes it, the native side lets
 * Android leave the app. `MainActivity` calls `window.__notiumHandleBack()`.
 */
export type BackHandler = () => boolean;

const handlers: BackHandler[] = [];

export function handleBack(): boolean {
  for (let i = handlers.length - 1; i >= 0; i--) {
    try {
      if (handlers[i]()) return true;
    } catch (e) {
      console.error("Back handler failed:", e);
    }
  }
  return false;
}

/** Registers a handler; returns the function that removes it. */
export function pushBackHandler(handler: BackHandler): () => void {
  handlers.push(handler);
  return () => {
    const i = handlers.lastIndexOf(handler);
    if (i >= 0) handlers.splice(i, 1);
  };
}

if (typeof window !== "undefined") {
  (window as unknown as { __notiumHandleBack?: () => boolean }).__notiumHandleBack = handleBack;
}
