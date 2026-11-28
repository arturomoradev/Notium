import { call } from "./ipc/commands";
import type { NoteSummaryDto } from "./ipc/types";
import docs from "./demoNotes.json";

export const DEMO_GUIDE_TITLE = "Guía Visual y Ejemplos de Notium";
export const LOREM_1_TITLE = "Lorem Ipsum - Parte 1";
export const LOREM_2_TITLE = "Lorem Ipsum - Parte 2";

// Generated once from the old Markdown demo notes with the editor's Tiptap schema.
const GUIDE_BODY = JSON.stringify(docs.guide);
const LOREM_1_BODY = JSON.stringify(docs.lorem1);
const LOREM_2_BODY = JSON.stringify(docs.lorem2);

async function upsertNoteByTitle(
  existing: NoteSummaryDto[],
  title: string,
  body: string,
  overwrite: boolean,
): Promise<string> {
  const found = existing.find((n) => n.title.toLowerCase() === title.toLowerCase());
  if (found) {
    if (overwrite) {
      const read = await call("note_read", { noteId: found.note_id });
      await call("note_save", {
        noteId: found.note_id,
        content: body,
        baseRev: read.rev,
      });
    }
    return found.note_id;
  }
  const created = await call("note_create", {
    kind: "tiptap",
    title,
    folder: null,
  });
  await call("note_save", {
    noteId: created.note.note_id,
    content: body,
    baseRev: created.rev,
  });
  return created.note.note_id;
}

export async function ensureDemoNotes(
  existing: NoteSummaryDto[],
  overwrite = false,
): Promise<string> {
  await upsertNoteByTitle(existing, LOREM_2_TITLE, LOREM_2_BODY, overwrite);
  await upsertNoteByTitle(existing, LOREM_1_TITLE, LOREM_1_BODY, overwrite);
  const guideId = await upsertNoteByTitle(existing, DEMO_GUIDE_TITLE, GUIDE_BODY, overwrite);
  return guideId;
}
