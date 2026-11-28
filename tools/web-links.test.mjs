import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { initCore } from "../src/web/core.ts";
import {
  backlinks,
  linkReferences,
  outlinks,
  repairLinks,
  snippetAround,
  unlinkedMentions,
} from "../src/web/backend/links.ts";
import { newDoc, stitchDoc, titleOf } from "../src/web/backend/state.ts";
import { attachmentPath } from "../src/web/backend/attachments.ts";

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

const note = (id, title, body) => ({
  id,
  doc: file(id, body),
  path: `notes/tiptap/${title}.json`,
  rev: 0,
  updated: 0,
  deleted: false,
});
const pathOf = (n) => `notes/tiptap/${titleOf(n)}.json`;
const a = note("a", "Proyecto Alfa", "Plan general.\n");
const b = note("b", "Ideas", "Ver [[proyecto alfa#Plan]] y ![[foto.webp]]\n");
const c = note("c", "Diario", "Hoy avancé en Proyecto Alfa sin enlazarlo.\n");
const notes = [a, b, c];

test("backlinks resolve case/diacritic-insensitively and ignore anchors", () => {
  const bl = backlinks(notes, pathOf, "a");
  assert.deepEqual(
    bl.map((x) => [x.source_id, x.kind]),
    [["b", "wikilink"]],
  );
});

test("outlinks mark resolved notes and synced attachments", () => {
  const out = outlinks(notes, pathOf, b, { "attachments/foto.webp": "h1" });
  const wiki = out.find((o) => !o.is_attachment);
  const att = out.find((o) => o.is_attachment);
  assert.equal(wiki.target_id, "a");
  assert.equal(att.is_resolved, true);
});

test("unlinked mentions exclude linking notes and the note itself", () => {
  assert.deepEqual(
    unlinkedMentions(notes, pathOf, "a", "Proyecto Alfa").map((m) => m.source_id),
    ["c"],
  );
});

test("link references and repair rewrite only wikilinks", () => {
  assert.equal(linkReferences(notes, pathOf, "Proyecto Alfa")[0].note_id, "b");
  const { edits, result } = repairLinks(notes, "Proyecto Alfa", "Proyecto Beta");
  assert.equal(result.updated_links_count, 1);
  assert.match(edits[0].body, /"target":"Proyecto Beta#Plan"|\[\[Proyecto Beta#Plan\]\]/);
});

test("stitch swaps the doc and keeps id like the native save", () => {
  const doc = JSON.parse(stitchDoc(a.doc, docText("# Nuevo"), "a", 42));
  assert.equal(doc.notium_id, "a");
  assert.equal(doc.updated, "42");
  assert.equal(doc.doc.content[0].content[0].text, "Nuevo");
  assert.throws(() => stitchDoc(a.doc, "# not json", "a", 43));
});

test("snippets and attachment names match the native shape", () => {
  assert.equal(snippetAround("x".repeat(50) + "Alfa", "alfa").startsWith("…"), true);
  assert.match(attachmentPath("Mi foto (1).webp"), /^attachments\/Mifoto1-[0-9a-f]{32}\.webp$/);
});
