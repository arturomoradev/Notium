interface Env {
  DB: D1Database;
  /** Bearer token secret (`wrangler secret put AUTH_TOKEN`). Required. */
  AUTH_TOKEN?: string;
  /** Set to "true" only for local development to run without a token. */
  ALLOW_UNAUTHENTICATED?: string;
}

function jsonError(status: number, error: string): Response {
  return new Response(JSON.stringify({ error }), {
    status,
    headers: { "Content-Type": "application/json" },
  });
}

/** Compare secrets in constant time (hashing first equalizes their lengths). */
async function timingSafeEqual(a: string, b: string): Promise<boolean> {
  const enc = new TextEncoder();
  const [ha, hb] = await Promise.all([
    crypto.subtle.digest("SHA-256", enc.encode(a)),
    crypto.subtle.digest("SHA-256", enc.encode(b)),
  ]);
  return crypto.subtle.timingSafeEqual(ha, hb);
}

/**
 * Bearer-token gate. Fails closed: a deployment without AUTH_TOKEN rejects every
 * request instead of exposing the vault, unless ALLOW_UNAUTHENTICATED is set.
 */
async function authorize(request: Request, env: Env): Promise<Response | null> {
  if (!env.AUTH_TOKEN) {
    if (env.ALLOW_UNAUTHENTICATED === "true") return null;
    return jsonError(
      503,
      "Servidor sin configurar: define el secreto AUTH_TOKEN (wrangler secret put AUTH_TOKEN)",
    );
  }
  const token = request.headers.get("Authorization")?.replace(/^Bearer\s+/i, "") ?? "";
  return (await timingSafeEqual(token, env.AUTH_TOKEN)) ? null : jsonError(401, "No autorizado");
}

const MAX_D1_BLOB_BYTES = 900_000;
/**
 * Largest note content accepted in one op. D1 refuses a value over 2 MB, and
 * one such op used to fail its whole push batch, blocking the device's sync.
 */
const MAX_OP_CONTENT_BYTES = 1_900_000;

const VAULT_ID_RE = /^[a-z0-9_-]{1,64}$/;

/** Vault partition from a request: empty means "default"; anything else must be valid. */
function parseVaultId(raw: string | null | undefined): string | null {
  const cleaned = (raw ?? "").trim().toLowerCase();
  if (!cleaned) return "default";
  return VAULT_ID_RE.test(cleaned) ? cleaned : null;
}

const invalidVault = () => jsonError(400, "vault_id inválido: usa 1-64 caracteres a-z, 0-9, _ o -");

function isSafeAttachmentPath(path: string): boolean {
  if (!path.startsWith("attachments/")) return false;
  if (path.includes("..") || path.includes("\\") || path.startsWith("/")) return false;
  return path.length > "attachments/".length;
}

let schemaInitialized = false;

async function ensureSchema(db: D1Database): Promise<void> {
  if (schemaInitialized) return;
  await db.batch([
    db.prepare(
      `CREATE TABLE IF NOT EXISTS journal (
        vault_id TEXT NOT NULL DEFAULT 'default',
        op_id TEXT NOT NULL,
        device_id TEXT NOT NULL,
        note_id TEXT NOT NULL,
        kind TEXT NOT NULL,
        base_rev INTEGER NOT NULL,
        new_rev INTEGER NOT NULL,
        content BLOB,
        hlc_ts INTEGER NOT NULL,
        hlc_cnt INTEGER NOT NULL,
        ts INTEGER NOT NULL,
        PRIMARY KEY (vault_id, op_id)
      )`,
    ),
    db.prepare(
      `CREATE TABLE IF NOT EXISTS blobs (
        vault_id TEXT NOT NULL DEFAULT 'default',
        hash TEXT NOT NULL,
        path TEXT NOT NULL,
        size INTEGER NOT NULL DEFAULT 0,
        data BLOB NOT NULL,
        ts INTEGER NOT NULL,
        PRIMARY KEY (vault_id, hash, path)
      )`,
    ),
  ]);
  // Columns added after the first release; "duplicate column" errors are expected.
  for (const [table, col] of [
    ["journal", "vault_id TEXT NOT NULL DEFAULT 'default'"],
    // Ruta del archivo que fija la op (crear, renombrar, mover). Opcional.
    ["journal", "path TEXT"],
    // Borrado definitivo desde la papelera de un dispositivo.
    ["journal", "purge INTEGER NOT NULL DEFAULT 0"],
    // Hash del contenido sobre el que se hizo la op (base del merge).
    ["journal", "base_hash TEXT"],
    // Orden de llegada: el cursor de /sync/pull. `ts` lo fija el inicio de la
    // petición, así que un push lento que confirma tarde quedaba detrás de un
    // cursor ya entregado y sus ops no llegaban nunca.
    ["journal", "seq INTEGER"],
    ["blobs", "seq INTEGER"],
  ]) {
    try {
      await db.prepare(`ALTER TABLE ${table} ADD COLUMN ${col}`).run();
    } catch {
      // Ya existe.
    }
  }
  await db.batch([
    // Rows from before `seq` keep their insertion order.
    db.prepare(`UPDATE journal SET seq = rowid WHERE seq IS NULL`),
    db.prepare(
      `UPDATE blobs SET seq = rowid + (SELECT COALESCE(MAX(seq), 0) FROM journal) WHERE seq IS NULL`,
    ),
    db.prepare(`CREATE INDEX IF NOT EXISTS idx_journal_vault_ts ON journal(vault_id, ts)`),
    db.prepare(`CREATE INDEX IF NOT EXISTS idx_blobs_vault_ts ON blobs(vault_id, ts)`),
    db.prepare(`CREATE INDEX IF NOT EXISTS idx_journal_seq ON journal(seq)`),
    db.prepare(`CREATE INDEX IF NOT EXISTS idx_blobs_seq ON blobs(seq)`),
    db.prepare(`CREATE INDEX IF NOT EXISTS idx_journal_vault_seq ON journal(vault_id, seq)`),
    db.prepare(`CREATE INDEX IF NOT EXISTS idx_blobs_vault_seq ON blobs(vault_id, seq)`),
  ]);
  schemaInitialized = true;
}

/** Same retention as the apps' trash (notium-core `TRASH_RETENTION_SECS`). */
const TRASH_RETENTION_SECS = 30 * 24 * 60 * 60;

/**
 * Erase a note's content history: every non-delete op keeps its row (so the
 * delivery order of later ops is untouched) but loses its content and path.
 */
function eraseNoteHistory(db: D1Database, vaultId: string, noteId: string): D1PreparedStatement {
  return db
    .prepare(
      `UPDATE journal SET content = NULL, path = NULL
       WHERE vault_id = ? AND note_id = ? AND lower(kind) != 'delete' AND content IS NOT NULL`,
    )
    .bind(vaultId, noteId);
}

/**
 * Erase the history of notes that have been in the trash longer than the
 * retention: their latest op is a delete received more than 30 days ago. The
 * server's clock decides (`ts`, ms), not the device's HLC, which a phone with
 * a wrong date could set decades in the past.
 */
async function purgeExpiredTrash(db: D1Database, nowSecs: number): Promise<number> {
  const { meta } = await db
    .prepare(
      `UPDATE journal SET content = NULL, path = NULL
       WHERE lower(kind) != 'delete' AND content IS NOT NULL
         AND (vault_id, note_id) IN (
           SELECT j.vault_id, j.note_id
           FROM journal j
           JOIN (SELECT vault_id, note_id, MAX(seq) AS seq FROM journal GROUP BY vault_id, note_id) last
             ON j.vault_id = last.vault_id AND j.note_id = last.note_id AND j.seq = last.seq
           WHERE lower(j.kind) = 'delete' AND j.ts < ?
         )`,
    )
    .bind((nowSecs - TRASH_RETENTION_SECS) * 1000)
    .run();
  return meta.changes ?? 0;
}

const contentLength = (op: any): number =>
  Array.isArray(op?.content) ? op.content.length : (op?.content?.byteLength ?? 0);

/**
 * Next value of the arrival sequence, shared by ops and blobs: one pull cursor
 * covers both, so neither may fall behind the other's numbers.
 */
const NEXT_SEQ = `(SELECT COALESCE(MAX(s), 0) + 1 FROM (SELECT MAX(seq) AS s FROM journal UNION ALL SELECT MAX(seq) FROM blobs))`;

/** Insert one op, ignoring a re-sent one: a retry must not be delivered again. */
function insertOp(db: D1Database, vaultId: string, op: any, serverTs: number): D1PreparedStatement {
  return db
    .prepare(
      `INSERT OR IGNORE INTO journal (vault_id, op_id, device_id, note_id, kind, base_rev, new_rev, content, hlc_ts, hlc_cnt, ts, path, purge, base_hash, seq)
       VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ${NEXT_SEQ})`,
    )
    .bind(
      vaultId,
      op.op_id,
      op.device_id,
      op.note_id,
      op.kind,
      op.base_rev,
      op.new_rev,
      op.content ? new Uint8Array(op.content) : null,
      op.hlc?.ts ?? 0,
      op.hlc?.cnt ?? 0,
      serverTs,
      typeof op.path === "string" ? op.path : null,
      op.purge === true ? 1 : 0,
      typeof op.base_hash === "string" ? op.base_hash : null,
    );
}

/** Statements that store `op` (and, for a permanent delete, erase its note's history). */
const opStatements = (db: D1Database, vaultId: string, op: any, serverTs: number) =>
  op.purge === true
    ? [insertOp(db, vaultId, op, serverTs), eraseNoteHistory(db, vaultId, op.note_id)]
    : [insertOp(db, vaultId, op, serverTs)];

const isValidOp = (op: any): boolean =>
  typeof op?.op_id === "string" &&
  typeof op.note_id === "string" &&
  typeof op.device_id === "string" &&
  typeof op.kind === "string" &&
  Number.isFinite(op.base_rev) &&
  Number.isFinite(op.new_rev) &&
  contentLength(op) <= MAX_OP_CONTENT_BYTES;

/** Pull cursor: `s:<seq>`; a bare number is a legacy server-time cursor (ms). */
function parseCursor(since: string): { seq: number } | { ts: number } {
  if (since.startsWith("s:")) return { seq: parseInt(since.slice(2), 10) || 0 };
  return { ts: parseInt(since, 10) || 0 };
}

const PULL_PAGE = 500;

export default {
  /** Daily cron (wrangler.jsonc `triggers`): empty the cloud trash. */
  async scheduled(_event: ScheduledController, env: Env): Promise<void> {
    await ensureSchema(env.DB);
    const erased = await purgeExpiredTrash(env.DB, Math.floor(Date.now() / 1000));
    console.log(`papelera: ${erased} ops de notas caducadas vaciadas`);
  },

  async fetch(request: Request, env: Env): Promise<Response> {
    const url = new URL(request.url);

    // 1. Verificación de Token Bearer (obligatoria)
    const denied = await authorize(request, env);
    if (denied) return denied;

    // 2. Health check
    if (url.pathname === "/sync/health") {
      return new Response(JSON.stringify({ status: "ok", engine: "cloudflare-d1" }), {
        headers: { "Content-Type": "application/json" },
      });
    }

    await ensureSchema(env.DB);

    // 3. Cloudflare D1: Sincronización de adjuntos binarios (/sync/blob/:hash)
    if (url.pathname.startsWith("/sync/blob/")) {
      const hash = url.pathname.replace("/sync/blob/", "").trim();
      const vaultId = parseVaultId(url.searchParams.get("vault_id"));
      if (!vaultId) return invalidVault();

      if (request.method === "POST" || request.method === "PUT") {
        const rawPath = url.searchParams.get("path") || `attachments/${hash}`;
        if (!isSafeAttachmentPath(rawPath)) {
          return jsonError(400, "Ruta de adjunto inválida; debe ubicarse en attachments/");
        }

        const body = await request.arrayBuffer();
        if (body.byteLength > MAX_D1_BLOB_BYTES) {
          return new Response(
            JSON.stringify({
              error: "Adjunto supera el límite de 900 KB por fila de Cloudflare D1",
              max_bytes: MAX_D1_BLOB_BYTES,
            }),
            { status: 413, headers: { "Content-Type": "application/json" } },
          );
        }

        // A re-upload of the same blob is ignored, so it is not announced again.
        const now = Date.now();
        await env.DB.prepare(
          `INSERT OR IGNORE INTO blobs (vault_id, hash, path, size, data, ts, seq)
           VALUES (?, ?, ?, ?, ?, ?, ${NEXT_SEQ})`,
        )
          .bind(vaultId, hash, rawPath, body.byteLength, new Uint8Array(body), now)
          .run();

        return new Response(
          JSON.stringify({ status: "stored", vault_id: vaultId, hash, path: rawPath, ts: now }),
          {
            status: 201,
            headers: { "Content-Type": "application/json" },
          },
        );
      }

      if (request.method === "GET") {
        const row = await env.DB.prepare(
          `SELECT data FROM blobs WHERE vault_id = ? AND hash = ? LIMIT 1`,
        )
          .bind(vaultId, hash)
          .first<{ data: ArrayBuffer | number[] }>();

        if (!row || !row.data) {
          return new Response("Blob no encontrado", { status: 404 });
        }

        const bytes =
          row.data instanceof ArrayBuffer
            ? new Uint8Array(row.data)
            : new Uint8Array(row.data as number[]);

        return new Response(bytes, {
          headers: { "Content-Type": "application/octet-stream" },
        });
      }
    }

    // 4. POST /sync/push: recepción y persistencia de mutaciones en SQLite D1
    if (url.pathname === "/sync/push" && request.method === "POST") {
      const body = await request.json<{ vault_id?: string; device_id: string; ops: any[] }>();
      const vaultId = parseVaultId(body.vault_id);
      if (!vaultId) return invalidVault();
      const ops = Array.isArray(body.ops) ? body.ops : [];
      const accepted: string[] = [];
      const rejected: string[] = [];
      const serverTs = Date.now();

      // D1 soporta transacciones batch atómicas de hasta 100 sentencias preparadas
      // Hasta 50 ops por lote: con sus posibles borrados definitivos no pasa de 100 sentencias.
      const BATCH_SIZE = 50;
      for (let i = 0; i < ops.length; i += BATCH_SIZE) {
        const chunk = ops.slice(i, i + BATCH_SIZE);
        const valid = chunk.filter(isValidOp);
        for (const op of chunk) if (!isValidOp(op)) rejected.push(String(op?.op_id ?? ""));
        try {
          await env.DB.batch(valid.flatMap((op) => opStatements(env.DB, vaultId, op, serverTs)));
          accepted.push(...valid.map((op) => op.op_id));
        } catch {
          // One bad op must not sink the rest: store them one by one.
          for (const op of valid) {
            try {
              await env.DB.batch(opStatements(env.DB, vaultId, op, serverTs));
              accepted.push(op.op_id);
            } catch (e) {
              console.log(`push: op ${op.op_id} rechazada: ${e}`);
              rejected.push(op.op_id);
            }
          }
        }
      }

      return new Response(
        JSON.stringify({
          status: "ok",
          vault_id: vaultId,
          accepted_op_ids: accepted,
          rejected_op_ids: rejected,
          server_cursor: serverTs.toString(),
        }),
        { headers: { "Content-Type": "application/json" } },
      );
    }

    // 5. GET /sync/pull: entrega de mutaciones y blobs remotos basados en cursor y vault_id
    if (url.pathname === "/sync/pull" && request.method === "GET") {
      const vaultId = parseVaultId(url.searchParams.get("vault_id"));
      if (!vaultId) return invalidVault();
      const since = parseCursor(url.searchParams.get("since") || "0");
      // Read before the page: a row that commits afterwards gets a higher seq.
      const top = await env.DB.prepare(
        `SELECT MAX(s) AS s FROM (SELECT MAX(seq) AS s FROM journal UNION ALL SELECT MAX(seq) FROM blobs)`,
      ).first<{ s: number | null }>();
      const where = "seq" in since ? "seq > ?" : "ts > ?";
      const from = "seq" in since ? since.seq : since.ts;

      const { results: opRows } = await env.DB.prepare(
        `SELECT op_id, device_id, note_id, kind, base_rev, new_rev, content, hlc_ts, hlc_cnt, ts, path, purge, base_hash, seq
         FROM journal
         WHERE vault_id = ? AND ${where}
         ORDER BY seq ASC
         LIMIT ${PULL_PAGE}`,
      )
        .bind(vaultId, from)
        .all();

      const { results: blobRows } = await env.DB.prepare(
        `SELECT hash, path, size, ts, seq
         FROM blobs
         WHERE vault_id = ? AND ${where}
         ORDER BY seq ASC
         LIMIT ${PULL_PAGE}`,
      )
        .bind(vaultId, from)
        .all();

      const ops = (opRows as any[]).map((row) => ({
        op_id: row.op_id,
        device_id: row.device_id,
        note_id: row.note_id,
        kind: row.kind,
        base_rev: row.base_rev,
        new_rev: row.new_rev,
        content: row.content ? Array.from(new Uint8Array(row.content)) : null,
        hlc: { ts: row.hlc_ts, cnt: row.hlc_cnt },
        ts: row.ts,
        // Omitted when absent, so the payload stays identical for older ops.
        ...(row.path ? { path: row.path } : {}),
        ...(row.purge ? { purge: true } : {}),
        ...(row.base_hash ? { base_hash: row.base_hash } : {}),
      }));
      const blobs = (blobRows as any[]).map((b) => ({
        hash: b.hash,
        path: b.path,
        size: b.size,
        ts: b.ts,
      }));

      // Every row has its own seq, so the cursor is the last seq delivered. A
      // full page stops at its last row (the other table's rows past it come
      // again next time; clients skip ops they already applied).
      const last = (rows: any[]) => rows[rows.length - 1]?.seq ?? 0;
      const full = [opRows, blobRows].filter((rows) => rows.length >= PULL_PAGE);
      const next = full.length
        ? Math.min(...full.map(last))
        : Math.max("seq" in since ? since.seq : (top?.s ?? 0), last(opRows), last(blobRows));

      return new Response(
        JSON.stringify({
          cursor: `s:${next}`,
          ops,
          blobs,
        }),
        { headers: { "Content-Type": "application/json" } },
      );
    }

    return new Response("Not Found", { status: 404 });
  },
};
