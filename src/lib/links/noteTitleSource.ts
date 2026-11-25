import { call } from "../ipc/commands";
import type { TitleSuggestionDto } from "../ipc/types";

// Cached, debounced-by-caller source for the `[[` note-link autocomplete.
// Fetches the full title list once (10 s TTL) and filters locally so the
// dropdown is instant and doesn't hammer the IPC bridge per keystroke.
let cache: { ts: number; items: TitleSuggestionDto[] } | null = null;
const TTL_MS = 10_000;

export async function noteTitleSource(prefix: string): Promise<TitleSuggestionDto[]> {
  if (!cache || Date.now() - cache.ts > TTL_MS) {
    const items = await call("note_suggest_titles", { prefix: "" });
    cache = { ts: Date.now(), items };
  }
  const p = prefix.toLowerCase();
  if (!p) return cache.items;
  return cache.items.filter((s) => s.title.toLowerCase().includes(p));
}
