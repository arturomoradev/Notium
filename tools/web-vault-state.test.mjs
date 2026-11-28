import { test } from "node:test";
import assert from "node:assert/strict";
import {
  applyRemote,
  emptyState,
  folderOp,
  localOp,
  newDoc,
  stitchDoc,
  tagsOf,
  titleOf,
  trashed,
} from "../src/web/backend/state.ts";
import { initCore } from "../src/web/core.ts";
import { readFileSync } from "node:fs";

// Bindings from `npm run build:wasm` (test:web builds them first).
await initCore(readFileSync(new URL("../src/web/wasm/pkg/notium_wasm_bg.wasm", import.meta.url)));

// A note file whose paragraphs (or `#` headings) are the lines of `text`.
const docText = (text) =>
  JSON.stringify({
    type: "doc",
    content: text
      .split("\n")
      .filter((l) => l.trim())
      .map((l) => {
        const m = /^(#{1,6}) (.*)$/.exec(l);
        return m
          ? {
              type: "heading",
              attrs: { level: m[1].length },
              content: [{ type: "text", text: m[2] }],
            }
          : { type: "paragraph", content: [{ type: "text", text: l }] };
      }),
  });
const file = (id, text, ts = 1) => stitchDoc(newDoc(id, ts), docText(text), id, ts);

const bytes = (s) => Array.from(new TextEncoder().encode(s));
const op = (o) => ({
  op_id: "op1",
  device_id: "pc",
  note_id: "n1",
  kind: "update",
  base_rev: 0,
  new_rev: 1,
  content: bytes(newDoc("n1", 10)),
  hlc: { ts: 10, cnt: 0 },
  ts: 1,
  ...o,
});

test("remote ops build notes, last one wins, delete hides", () => {
  const s = emptyState();
  applyRemote(
    s,
    [op({ kind: "Create", new_rev: 0 }), op({ content: bytes(file("n1", "# Otro")) })],
    "web",
  );
  assert.equal(titleOf(s.notes.n1), "Otro");
  assert.equal(s.notes.n1.rev, 1);
  applyRemote(s, [op({ kind: "delete", content: null, new_rev: 2 })], "web");
  assert.equal(s.notes.n1.deleted, true);
});

test("folder ops track empty folders and never become notes", () => {
  const s = emptyState();
  applyRemote(s, [op({ note_id: "folder:A/B", kind: "create", content: null, new_rev: 0 })], "web");
  assert.deepEqual(s.folders, { "A/B": true });
  assert.deepEqual(s.notes, {});
  applyRemote(s, [op({ note_id: "folder:A/B", kind: "delete", content: null })], "web");
  assert.deepEqual(s.folders, {});
  assert.deepEqual(s.notes, {});
  folderOp(s, "web", "C", "create");
  assert.deepEqual(s.folders, { C: true });
  assert.equal(s.pending[0].note_id, "folder:C");
  assert.equal(s.pending[0].content, null);
});

test("local save bumps rev and is queued; pending edit survives a concurrent remote op", () => {
  const s = emptyState();
  applyRemote(s, [op({})], "web");
  const doc = stitchDoc(s.notes.n1.doc, docText("# Web"), "n1", 99);
  localOp(s, "web", "n1", "update", doc, 1);
  assert.equal(s.pending[0].base_rev, 1);
  assert.equal(s.pending[0].new_rev, 2);
  assert.match(s.notes.n1.doc, /"updated": "99"/);
  assert.match(s.notes.n1.doc, /"notium_id": "n1"/);
  applyRemote(s, [op({ op_id: "op2", new_rev: 2, content: bytes(file("n1", "# PC")) })], "web");
  assert.equal(titleOf(s.notes.n1), "Web");
});

test("tags and title fallback", () => {
  const n = {
    id: "note_abcdefgh",
    doc: file("note_abcdefgh", "texto #idea y #a/b\n## no"),
    rev: 0,
    updated: 0,
    deleted: false,
  };
  assert.deepEqual(tagsOf(n), ["a/b", "idea"]); // native order (sorted)
  assert.equal(titleOf({ ...n, doc: "" }), "Nota note_abc");
});

test("paths: latest path op wins, titles come from the file name", () => {
  const s = emptyState();
  applyRemote(
    s,
    [
      op({ hlc: { ts: 20, cnt: 0 }, path: "notes/tiptap/Dir/Nuevo.json" }),
      op({ op_id: "op2", hlc: { ts: 15, cnt: 0 }, path: "notes/tiptap/Viejo.json" }),
      op({ op_id: "op3", hlc: { ts: 30, cnt: 0 }, path: "notes/tiptap/../fuera.json" }),
    ],
    "web",
  );
  assert.equal(s.notes.n1.path, "notes/tiptap/Dir/Nuevo.json");
  assert.equal(titleOf(s.notes.n1), "Nuevo");
  // A local rename outranks what the web has seen and travels with the op.
  localOp(s, "web", "n1", "update", s.notes.n1.doc, 1, "notes/tiptap/Final.json");
  assert.equal(s.pending.at(-1).path, "notes/tiptap/Final.json");
  applyRemote(
    s,
    [op({ op_id: "op4", hlc: { ts: 21, cnt: 0 }, path: "notes/tiptap/X.json" })],
    "web",
  );
  assert.equal(titleOf(s.notes.n1), "Final");
});

test("own ops rebuild a fresh state but never roll back a newer local one", () => {
  const fresh = emptyState();
  applyRemote(fresh, [op({ device_id: "web", path: "notes/tiptap/Mia.json" })], "web");
  assert.equal(titleOf(fresh.notes.n1), "Mia");
  const s = emptyState();
  applyRemote(s, [op({})], "web");
  localOp(s, "web", "n1", "update", file("n1", "# Nueva"), 1);
  applyRemote(
    s,
    [op({ op_id: "old", device_id: "web", new_rev: 1, content: bytes(file("n1", "# Vieja")) })],
    "web",
  );
  assert.equal(titleOf(s.notes.n1), "Nueva");
});

test("deletes stick: stale, concurrent or repeated ops never revive a note", () => {
  const s = emptyState();
  applyRemote(
    s,
    [
      op({ new_rev: 1 }),
      op({ op_id: "del", kind: "delete", base_rev: 1, new_rev: 2, content: null }),
    ],
    "web",
  );
  assert.equal(s.notes.n1.deleted, true);
  // Same op delivered again by an overlapping pull page, and a concurrent edit.
  applyRemote(
    s,
    [op({ new_rev: 1 }), op({ op_id: "late", device_id: "phone", base_rev: 1, new_rev: 2 })],
    "web",
  );
  assert.equal(s.notes.n1.deleted, true);
  // A delete that arrives before the note is known still blocks it.
  applyRemote(
    s,
    [op({ note_id: "n2", kind: "delete", new_rev: 1, content: null }), op({ note_id: "n2" })],
    "web",
  );
  assert.equal(s.notes.n2.deleted, true);
});

test("an older content op does not overwrite newer content", () => {
  const s = emptyState();
  applyRemote(
    s,
    [
      op({ new_rev: 3, content: bytes(file("n1", "# Nueva")) }),
      op({ op_id: "old", new_rev: 2, content: bytes(file("n1", "# Vieja")) }),
    ],
    "web",
  );
  assert.equal(titleOf(s.notes.n1), "Nueva");
});

test("trash: deletes keep the content, restores revive it, purges erase it", () => {
  const s = emptyState();
  applyRemote(s, [op({ new_rev: 1 })], "web");
  applyRemote(
    s,
    [
      op({
        op_id: "d",
        kind: "delete",
        base_rev: 1,
        new_rev: 2,
        content: null,
        hlc: { ts: 100, cnt: 0 },
      }),
    ],
    "web",
  );
  assert.deepEqual(
    trashed(s, 200).map((n) => [n.id, n.deletedAt]),
    [["n1", 100]],
  );
  assert.equal(trashed(s, 100 + 31 * 86400).length, 0, "expired after 30 days");

  // A restore from another device builds on the tombstone rev.
  applyRemote(
    s,
    [
      op({
        op_id: "r",
        base_rev: 2,
        new_rev: 3,
        path: "notes/tiptap/Vuelta.json",
        hlc: { ts: 150, cnt: 0 },
      }),
    ],
    "web",
  );
  assert.equal(s.notes.n1.deleted, false);
  assert.equal(titleOf(s.notes.n1), "Vuelta");

  // Local delete, then a local restore op carries base = tombstone rev.
  localOp(s, "web", "n1", "delete", null, 3);
  assert.equal(trashed(s).length, 1);
  localOp(s, "web", "n1", "update", s.notes.n1.doc, s.notes.n1.rev, "notes/tiptap/Vuelta.json");
  assert.equal(s.pending.at(-1).base_rev, 4);
  assert.equal(s.notes.n1.deleted, false);

  // Purge from another device: no longer restorable anywhere.
  applyRemote(
    s,
    [op({ op_id: "d2", kind: "delete", base_rev: 5, new_rev: 6, content: null })],
    "web",
  );
  applyRemote(
    s,
    [op({ op_id: "p", kind: "delete", base_rev: 6, new_rev: 7, content: null, purge: true })],
    "web",
  );
  assert.equal(s.notes.n1.doc, "");
  assert.equal(trashed(s).length, 0);
});

test("local edits name the version they replace; far-future clocks are not adopted", async () => {
  const { contentHash } = await import("../src/web/core.ts");
  const s = emptyState();
  applyRemote(s, [op({})], "web");
  const before = s.notes.n1.doc;
  localOp(s, "web", "n1", "update", "# nuevo\n", s.notes.n1.rev);
  assert.equal(s.pending.at(-1).base_hash, contentHash(new TextEncoder().encode(before)));
  localOp(s, "web", "n2", "create", "# otra\n", 0);
  assert.equal(s.pending.at(-1).base_hash, undefined);

  const clock = { ...s.clock };
  const future = Math.floor(Date.now() / 1000) + 10 * 24 * 3600;
  applyRemote(s, [op({ op_id: "f", note_id: "n3", hlc: { ts: future, cnt: 0 } })], "web");
  assert.deepEqual(s.clock, clock);
});

test("names arrive and are made in NFC, like on the devices", async () => {
  const { sanitizeTitle, sanitizeFolderPath } = await import("../src/web/core.ts");
  assert.equal(sanitizeTitle("Café"), "Café");
  assert.equal(sanitizeFolderPath("México/x"), "México/x");
  const s = emptyState();
  applyRemote(s, [op({ path: "notes/tiptap/Café.json" })], "web");
  assert.equal(s.notes.n1.path, "notes/tiptap/Café.json");
  applyRemote(s, [op({ op_id: "f1", note_id: "folder:México", content: null })], "web");
  assert.ok(s.folders["México"]);
});
