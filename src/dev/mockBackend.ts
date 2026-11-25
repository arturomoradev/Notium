/**
 * Dev-only fake backend so the UI can be previewed in a plain browser
 * (`npm run dev` without Tauri), e.g. to check layouts at different sizes.
 * Loaded from `main.ts` only in dev builds when the Tauri IPC is absent; add
 * `?mobile` to the URL to preview the phone/tablet UI. Nothing is persisted.
 */
import { mockIPC } from "@tauri-apps/api/mocks";
import type { NoteSummaryDto } from "../lib/ipc/types";

const GUIDE = `# Guía Visual y Ejemplos de Notium

Esta nota sirve como **catálogo visual** de las funciones de *Live Preview*.

Etiquetas de prueba: #guia #markdown #notium

## Listas y tareas

- Elemento con \`código\` y un [[Proyecto Alfa]]
- [x] Tarea completada
- [ ] Tarea pendiente

> La simplicidad es la máxima sofisticación.

> [!tip] Consejo
> Los callouts usan el color de su tipo.

| Columna | Valor |
| --- | --- |
| Ancho | 700px |

\`\`\`python
print("hello world")
\`\`\`
`;

const titles = [
  "Guía Visual y Ejemplos de Notium",
  "Proyecto Alfa",
  "Reunión semanal con un título bastante largo para probar el recorte",
  "Ideas",
];
const notes: NoteSummaryDto[] = titles.map((title, i) => ({
  note_id: `n${i}`,
  title,
  format: "markdown",
  path: i === 1 ? `Proyectos/${title}.md` : `${title}.md`,
  updated_at: null,
  tags: i === 0 ? ["guia", "markdown", "notium"] : [],
}));
const contents = new Map(
  notes.map((n) => [n.note_id, n.note_id === "n0" ? GUIDE : `# ${n.title}\n`]),
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
