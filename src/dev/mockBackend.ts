/**
 * Dev-only fake backend so the UI can be previewed in a plain browser
 * (`npm run dev` without Tauri), e.g. to check layouts at different sizes.
 * Loaded from `main.ts` only in dev builds when the Tauri IPC is absent; add
 * `?mobile` to the URL to preview the phone/tablet UI. Nothing is persisted.
 */
import { mockIPC } from "@tauri-apps/api/mocks";
import type { NoteSummaryDto } from "../lib/ipc/types";
import docs from "../lib/demoNotes.json";

const heading = (text: string) => ({
  type: "doc",
  content: [{ type: "heading", attrs: { level: 1 }, content: [{ type: "text", text }] }],
});

const titles = [
  "Guía Visual y Ejemplos de Notium",
  "Proyecto Alfa",
  "Reunión semanal con un título bastante largo para probar el recorte",
  "Ideas",
];
const notes: NoteSummaryDto[] = titles.map((title, i) => ({
  note_id: `n${i}`,
  title,
  format: "tiptap",
  path: i === 1 ? `Proyectos/${title}.json` : `${title}.json`,
  updated_at: null,
  tags: i === 0 ? ["guia", "markdown", "notium"] : [],
}));
const contents = new Map(
  notes.map((n) => [n.note_id, JSON.stringify(n.note_id === "n0" ? docs.guide : heading(n.title))]),
);

const syncStatus = {
  enabled: false,
  server_url: "",
  device_id: "dev",
  vault_id: "default",
  state: "offline",
  last_synced_at: null,
  pending_ops: 0,
  last_error: null,
};

export function installMockBackend(): void {
  mockIPC(
    (cmd, payload) => {
      const args = (payload ?? {}) as Record<string, unknown>;
      switch (cmd) {
        case "vault_init":
          return {
            root: "/mock",
            name: "Demo",
            notes: notes.length,
            attachments: 0,
            folders: 1,
            opened: true,
          };
        case "vault_get_settings":
          return { source_mode: false, theme: "dark", font_size: 15, image_quality: "1080p" };
        case "vault_list_folders":
          return [{ path: "Proyectos" }];
        case "vault_list_profiles":
          return [{ name: "Demo", path: "/mock", active: true }];
        case "note_list":
          return notes;
        case "note_read": {
          const note = notes.find((n) => n.note_id === args.noteId) ?? notes[0];
          return { note, content: contents.get(note.note_id) ?? "", rev: 1 };
        }
        case "note_save": {
          const note = notes.find((n) => n.note_id === args.noteId) ?? notes[0];
          contents.set(note.note_id, String(args.content ?? ""));
          return { rev: Number(args.baseRev ?? 1) + 1, note };
        }
        case "note_get_backlinks":
          return args.noteId === "n1"
            ? [
                {
                  source_id: "n0",
                  source_title: titles[0],
                  link_text: "Proyecto Alfa",
                  kind: "wiki",
                  snippet: "Elemento con código y un [[Proyecto Alfa]]",
                },
              ]
            : [];
        case "get_sync_status":
        case "sync_now":
        case "pause_sync":
        case "resume_sync":
          return syncStatus;
        default:
          // List-style commands get an empty list; everything else resolves to null.
          return /_list|search|backlinks|outlinks|mentions|suggest|references/.test(cmd)
            ? []
            : null;
      }
    },
    { shouldMockEvents: true },
  );
}
