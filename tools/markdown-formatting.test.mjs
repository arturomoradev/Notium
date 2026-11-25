import test from "node:test";
import assert from "node:assert/strict";
import {
  editMarkdownTable,
  insertMarkdown,
  isolateMarkdownFrontmatter,
  stitchMarkdownFrontmatter,
  toggleMarkdownPrefix,
  wrapMarkdownSelection,
} from "../src/components/editors/markdownFormatting.ts";
import { MAIN_DEFS, buildMoreActions } from "../src/components/editors/toolbar.ts";

test("toolbar formatting wraps at the current selection and leaves an empty cursor inside markers", () => {
  for (const [name, marker] of [
    ["bold", "**"],
    ["italic", "*"],
    ["strike", "~~"],
    ["highlight", "=="],
    ["code", "`"],
  ]) {
    const empty = wrapMarkdownSelection("before after", 7, 7, marker);
    assert.equal(empty.doc, `before ${marker}${marker}after`, name);
    assert.equal(empty.anchor, 7 + marker.length, name);
    const selected = wrapMarkdownSelection("before text after", 7, 11, marker);
    assert.equal(selected.doc, `before ${marker}text${marker} after`, name);
  }
});

test("internal link and embed insert both delimiters with cursor at +2 and +3", () => {
  const link = insertMarkdown("abc", 1, 1, "[[]]", 2);
  assert.deepEqual(link, { doc: "a[[]]bc", anchor: 3, head: 3 });
  const embed = insertMarkdown("abc", 1, 1, "![[]]", 3);
  assert.deepEqual(embed, { doc: "a![[]]bc", anchor: 4, head: 4 });
});

test("tag, heading, quote, external link, and attachment insertion remain at the cursor", () => {
  assert.equal(insertMarkdown("xy", 1, 1, "#", 1).doc, "x#y");
  assert.equal(toggleMarkdownPrefix("first\nsecond", 8, 8, "# ").doc, "first\n# second");
  assert.equal(toggleMarkdownPrefix("quote", 2, 2, "> ").doc, "> quote");
  assert.equal(insertMarkdown("xy", 1, 1, "[texto](https://)", 1).doc, "x[texto](https://)y");
  assert.equal(
    insertMarkdown("xy", 1, 1, "![[attachments/a.png]]", 22).doc,
    "x![[attachments/a.png]]y",
  );
});

test("bullet, numbered, and task prefixes toggle on the selected line", () => {
  for (const prefix of ["- ", "1. ", "- [ ] "]) {
    const added = toggleMarkdownPrefix("top\ntarget", 6, 6, prefix);
    assert.equal(added.doc, `top\n${prefix}target`);
    const removed = toggleMarkdownPrefix(added.doc, added.anchor, added.head, prefix);
    assert.equal(removed.doc, "top\ntarget");
  }
});

test("indent and outdent operate on leading spaces rather than byte zero", () => {
  const indented = toggleMarkdownPrefix("one\ntwo", 5, 5, "  ");
  assert.equal(indented.doc, "one\n  two");
  const outdented = toggleMarkdownPrefix(indented.doc, indented.anchor, indented.head, "  ");
  assert.equal(outdented.doc, "one\ntwo");
});

test("Markdown table row/column tools add and remove cells around the cursor", () => {
  const table = "| A | B |\n| --- | --- |\n| x | y |";
  const position = table.lastIndexOf("x") + 1;
  assert.equal(editMarkdownTable(table, position, "add-row")?.doc.split("\n").length, 4);
  assert.equal(
    editMarkdownTable(table, position, "add-column")?.doc,
    "| A | B |  |\n| --- | --- | --- |\n| x | y |  |",
  );
  assert.equal(editMarkdownTable(table, position, "delete-row")?.doc, "| A | B |\n| --- | --- |");
  assert.equal(editMarkdownTable(table, position, "delete-column")?.doc, "| B |\n| --- |\n| y |");
  assert.equal(editMarkdownTable(table, position, "delete-table")?.doc, "");
});

test("frontmatter envelope is isolated from byte zero and exactly re-stitched", () => {
  const original = "---\r\ntitle: Keep\r\nnotium_id: id-1\r\nunknown: preserve\r\n---\r\n# body";
  const { envelope, body } = isolateMarkdownFrontmatter(original);
  assert.equal(body, "# body");
  assert.ok(envelope.startsWith("---\r\ntitle: Keep\r\n"));
  assert.equal(stitchMarkdownFrontmatter(original, "# changed", "id-1"), envelope + "# changed");
});

test("undo, redo, read-only, heading, and all toolbar operation identifiers are represented", () => {
  assert.equal(MAIN_DEFS.length, 19);
  assert.deepEqual(
    MAIN_DEFS.map((action) => action.id),
    [
      "undo",
      "redo",
      "note-link",
      "embed",
      "tag",
      "attachment",
      "heading",
      "bold",
      "italic",
      "strike",
      "highlight",
      "code",
      "quote",
      "link",
      "bullet-list",
      "ordered-list",
      "checklist",
      "indent",
      "outdent",
    ],
  );
  assert.deepEqual(
    buildMoreActions(() => undefined, "markdown").map((action) => action.id),
    ["table", "codeblock", "hr", "addrow", "addcol", "delrow", "delcol", "deltable"],
  );
});
