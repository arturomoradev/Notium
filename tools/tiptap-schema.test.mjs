import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import { getSchema } from "@tiptap/core";
import { Node } from "@tiptap/pm/model";
import { noteExtensions } from "../src/components/editors/tiptapExtensions.ts";
import docs from "../src/lib/demoNotes.json" with { type: "json" };

const schema = getSchema(noteExtensions());
const valid = (doc) => Node.fromJSON(schema, doc).check();

test("demo notes are valid docs for the editor schema", () => {
  for (const [name, doc] of Object.entries(docs)) assert.doesNotThrow(() => valid(doc), name);
});

test("the core's note fixtures load in the editor", () => {
  const dir = new URL("../crates/notium-core/tests/fixtures/notes/", import.meta.url);
  for (const f of fs.readdirSync(dir)) {
    const note = JSON.parse(fs.readFileSync(new URL(f, dir), "utf8"));
    assert.doesNotThrow(() => valid(note.doc), f);
  }
});

test("wiki links keep target and embed through JSON", () => {
  const doc = {
    type: "doc",
    content: [
      {
        type: "paragraph",
        content: [
          { type: "wikiLink", attrs: { target: "Nota#Sección|alias", embed: false } },
          { type: "wikiLink", attrs: { target: "img.png", embed: true } },
        ],
      },
    ],
  };
  assert.deepEqual(JSON.parse(JSON.stringify(Node.fromJSON(schema, doc).toJSON())), doc);
});

test("linkFirstMention links one whole-word mention, outside code", async () => {
  const { linkFirstMention } = await import("../src/lib/plainText.ts");
  const p = (...content) => ({ type: "paragraph", content });
  const t = (text, marks) => (marks ? { type: "text", text, marks } : { type: "text", text });
  const doc = JSON.stringify({
    type: "doc",
    content: [p(t("Destinos y ")), p(t("Destino", [{ type: "code" }])), p(t("ver destino hoy"))],
  });
  const out = JSON.parse(linkFirstMention(doc, "Destino"));
  assert.deepEqual(out.content[2].content, [
    t("ver "),
    { type: "wikiLink", attrs: { target: "Destino", embed: false } },
    t(" hoy"),
  ]);
  assert.equal(linkFirstMention(JSON.stringify(out), "Nada"), null);
  assert.doesNotThrow(() => valid(out));
});

test("math written as text becomes math nodes, code and prices stay text", async () => {
  const { migrateNote: migrateMath } = await import("../src/components/editors/noteMigration.ts");
  const p = (...content) => ({ type: "paragraph", content });
  const t = (text, marks) => (marks ? { type: "text", text, marks } : { type: "text", text });
  const doc = {
    type: "doc",
    content: [
      p(t("Masa $E = mc^2$ y precio $100 o $5$.")),
      p(t("$$\n\sum_i i\n$$")),
      p(t("$x$", [{ type: "code" }])),
      { type: "codeBlock", attrs: { language: "latex" }, content: [t("a^2")] },
      { type: "codeBlock", attrs: { language: "js" }, content: [t("let a = $b$")] },
    ],
  };
  assert.equal(migrateMath(doc), true);
  assert.deepEqual(doc.content[0].content, [
    t("Masa "),
    { type: "inlineMath", attrs: { latex: "E = mc^2" } },
    t(" y precio $100 o $5$."),
  ]);
  assert.deepEqual(doc.content[1], { type: "blockMath", attrs: { latex: "\sum_i i" } });
  assert.deepEqual(doc.content[2].content, [t("$x$", [{ type: "code" }])]);
  assert.deepEqual(doc.content[3], { type: "blockMath", attrs: { latex: "a^2" } });
  assert.equal(doc.content[4].content[0].text, "let a = $b$");
  assert.doesNotThrow(() => valid(doc));
  assert.equal(migrateMath(doc), false);
});

test("quotes starting with [!type] become callouts, merged ones are split", async () => {
  const { migrateNote } = await import("../src/components/editors/noteMigration.ts");
  const p = (...content) => ({ type: "paragraph", content });
  const t = (text, marks) => (marks ? { type: "text", text, marks } : { type: "text", text });
  const doc = {
    type: "doc",
    content: [
      {
        type: "blockquote",
        content: [
          p(t("Cita normal")),
          p(t("[!tip] Consejo\ncuerpo del consejo")),
          p(t("[!warning]- Plegable")),
          p(t("más texto")),
        ],
      },
      { type: "blockquote", content: [p(t("sin marcador"))] },
    ],
  };
  assert.equal(migrateNote(doc), true);
  assert.deepEqual(
    doc.content.map((n) => [n.type, n.attrs?.type, n.attrs?.title, n.attrs?.fold]),
    [
      ["blockquote", undefined, undefined, undefined],
      ["callout", "tip", "Consejo", ""],
      ["callout", "warning", "Plegable", "-"],
      ["blockquote", undefined, undefined, undefined],
    ],
  );
  assert.deepEqual(doc.content[1].content, [p(t("cuerpo del consejo"))]);
  assert.deepEqual(doc.content[2].content, [p(t("más texto"))]);
  assert.doesNotThrow(() => valid(doc));
  assert.equal(migrateNote(doc), false);
});
