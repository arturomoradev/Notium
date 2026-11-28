// Typed wrapper over crates/notium-wasm: the same Rust code the native app uses
// for titles, tags, links, hashes and merges. Generate the bindings with
// `npm run build:wasm` (output in ./wasm/pkg, not committed).
import init, * as wasm from "./wasm/pkg/notium_wasm.js";

export type LinkKind = "wikilink" | "embed" | "markdownlink";

export interface OutLink {
  text: string;
  kind: LinkKind;
  target: string;
  is_attachment: boolean;
}

export interface Extracted {
  title: string;
  tags: string[];
  plain_text: string;
  out_links: OutLink[];
  attachment_refs: string[];
}

/**
 * Load the WebAssembly module. In the browser it fetches the bundled `.wasm`;
 * tests pass the bytes. Must resolve before any other function is called.
 */
export async function initCore(module?: BufferSource): Promise<void> {
  await (module ? init({ module_or_path: module }) : init());
}

const parse = <T>(json: string): T => JSON.parse(json) as T;

/** Extract a note file (Tiptap JSON envelope). */
export const extractNote = (noteId: string, text: string): Extracted =>
  parse(wasm.extract_note(noteId, text));

export const contentHash = (bytes: Uint8Array): string => wasm.content_hash(bytes);

/** The Tiptap doc of a note file, as JSON (an empty doc when there is none). */
export const docJson = (text: string): string => wasm.doc_json(text);

/** The note file with its doc replaced; throws on a malformed doc. */
export const withDoc = (previous: string, doc: string): string => wasm.with_doc(previous, doc);

/** A new, empty note file. */
export const newNote = (id: string, created: string): string => wasm.new_note(id, created);

export const injectNotiumId = (text: string, id: string): string => wasm.inject_notium_id(text, id);

export const upsertField = (text: string, key: string, val: string): string =>
  wasm.upsert_field(text, key, val);

export const normalizeTitle = (s: string): string => wasm.normalize_title(s);

/** NFC like the native build (`paths::nfc`), so a note has one path everywhere. */
export const sanitizeTitle = (title: string): string => wasm.sanitize_title(title.normalize("NFC"));

/** Folder path relative to `notes/tiptap`; throws on `.`/`..` segments. */
export const sanitizeFolderPath = (folder: string): string =>
  wasm.sanitize_folder_path(folder.normalize("NFC"));

export const titleFromPath = (relPath: string): string => wasm.title_from_path(relPath);

export const isSafeNotePath = (path: string): boolean => wasm.is_safe_note_rel_path(path);

/** `[content, count]`. */
export const rewriteNoteLinks = (
  content: string,
  oldTitle: string,
  newTitle: string,
): [string, number] => parse(wasm.rewrite_note_links(content, oldTitle, newTitle));

/** `[merged, conflict]`. */
export const mergeMd = (base: string, mine: string, theirs: string): [string, boolean] =>
  parse(wasm.merge_md(base, mine, theirs));
