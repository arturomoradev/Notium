// Sync server (server/cloudflare-sync) against an in-memory D1 stand-in built
// on node:sqlite. Each test reproduces a failure found in the sync audit.
import { test } from "node:test";
import assert from "node:assert/strict";
import { DatabaseSync } from "node:sqlite";
import worker from "../server/cloudflare-sync/src/index.ts";

/** D1's per-value limit (string, BLOB or row): 2,000,000 bytes. */
const D1_MAX_VALUE = 2_000_000;

/** The subset of the D1 API the worker uses, with a hook to pause a batch. */
function fakeD1() {
  const db = new DatabaseSync(":memory:");
  const exec = (sql, params) => {
    if (d1.failOn && params.includes(d1.failOn)) throw new Error("D1_ERROR: injected failure");
    for (const p of params) {
      if ((p?.byteLength ?? p?.length ?? 0) > D1_MAX_VALUE && typeof p !== "number") {
        throw new Error("D1_ERROR: string or blob too big: SQLITE_TOOBIG");
      }
    }
    return db.prepare(sql);
  };
  const stmt = (sql, params = []) => ({
    sql,
    params,
    bind: (...p) => stmt(sql, p),
    run: async () => ({ meta: { changes: Number(exec(sql, params).run(...params).changes) } }),
    all: async () => ({ results: exec(sql, params).all(...params) }),
    first: async () => exec(sql, params).get(...params) ?? null,
  });
  const d1 = {
    /** Set to a promise to pause the next batch until it resolves. */
    hold: null,
    reached: null,
    prepare: (sql) => stmt(sql),
    async batch(stmts) {
      if (d1.hold) {
        const hold = d1.hold;
        d1.hold = null;
        d1.reached?.();
        await hold;
      }
      db.exec("BEGIN");
      try {
        const out = stmts.map((s) => ({ results: exec(s.sql, s.params).all(...s.params) }));
        db.exec("COMMIT");
        return out;
      } catch (e) {
        db.exec("ROLLBACK");
        throw e;
      }
    },
  };
  return d1;
}

const env = { DB: fakeD1(), ALLOW_UNAUTHENTICATED: "true" };
const realNow = Date.now;
const at = (ms) => (Date.now = () => ms);

const bytes = (s) => Array.from(new TextEncoder().encode(s));
let opSeq = 0;
const op = (o = {}) => ({
  op_id: `op_${++opSeq}`,
  device_id: "pc",
  note_id: "n1",
  kind: "update",
  base_rev: 0,
  new_rev: 1,
  content: bytes("# hola\n"),
  hlc: { ts: Math.floor(realNow() / 1000), cnt: 0 },
  ts: 0,
  ...o,
});

async function push(vault, ops) {
  const res = await worker.fetch(
    new Request("https://sync.test/sync/push", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ vault_id: vault, device_id: "pc", ops }),
    }),
    env,
  );
  return { status: res.status, body: await res.json() };
}

async function pull(vault, since = "0") {
  const q = `vault_id=${encodeURIComponent(vault)}&since=${encodeURIComponent(since)}`;
  const res = await worker.fetch(new Request(`https://sync.test/sync/pull?${q}`), env);
  assert.equal(res.status, 200);
  return res.json();
}

/** Pull from `since` until caught up, like the clients do. */
async function pullAll(vault, since = "0") {
  const ops = [];
  for (;;) {
    const page = await pull(vault, since);
    ops.push(...page.ops);
    if (page.cursor === since || page.ops.length < 500) return { ops, cursor: page.cursor };
    since = page.cursor;
  }
}

// #1: a push that commits after a later push was already pulled must still be delivered.
test("slow push committing late is not skipped by the cursor", async () => {
  await pull("race"); // creates the schema
  let release;
  env.DB.hold = new Promise((r) => (release = r));
  const reached = new Promise((r) => (env.DB.reached = r));

  at(1_000);
  const slow = push("race", [op({ op_id: "slow" })]);
  await reached; // the slow push is stamped and waiting on D1
  at(2_000);
  await push("race", [op({ op_id: "fast" })]);
  const first = await pullAll("race");
  assert.deepEqual(
    first.ops.map((o) => o.op_id),
    ["fast"],
  );

  release();
  await slow;
  const second = await pullAll("race", first.cursor);
  Date.now = realNow;
  assert.deepEqual(
    second.ops.map((o) => o.op_id),
    ["slow"],
    "the late op was skipped",
  );
});

// #9: retrying a push (lost response, restored backup) must not re-broadcast the op.
test("a re-sent op is stored once and not delivered again", async () => {
  const o = op({ op_id: "dup" });
  await push("dup", [o]);
  const { cursor } = await pullAll("dup");
  await push("dup", [o]);
  const again = await pullAll("dup", cursor);
  assert.deepEqual(again.ops, [], "the retry was delivered again");
});

// #6: one oversized op must not fail the whole push (and with it the sync).
test("an oversized op is rejected alone; the rest is accepted", async () => {
  const small = op({ op_id: "small" });
  const huge = op({ op_id: "huge", content: new Array(D1_MAX_VALUE + 10).fill(65) });
  const res = await push("big", [small, huge]);
  assert.equal(res.status, 200, JSON.stringify(res.body).slice(0, 200));
  assert.deepEqual(res.body.accepted_op_ids, ["small"]);
  assert.deepEqual(res.body.rejected_op_ids, ["huge"]);
});

// #6: an op D1 refuses for any reason is rejected alone (batch retried op by op).
test("a batch D1 refuses is retried op by op", async () => {
  env.DB.failOn = "poison";
  const res = await push("batchfail", [
    op({ op_id: "ok1" }),
    op({ op_id: "poison" }),
    op({ op_id: "ok2" }),
  ]);
  env.DB.failOn = null;
  assert.deepEqual(res.body.accepted_op_ids, ["ok1", "ok2"]);
  assert.deepEqual(res.body.rejected_op_ids, ["poison"]);
  const { ops } = await pullAll("batchfail");
  assert.deepEqual(
    ops.map((o) => o.op_id),
    ["ok1", "ok2"],
  );
});

// Clients updated from a server-time cursor (a bare number) carry on without gaps.
test("a legacy numeric cursor still delivers what came after it", async () => {
  at(5_000);
  await push("legacy", [op({ op_id: "old" })]);
  at(9_000);
  await push("legacy", [op({ op_id: "new" })]);
  Date.now = realNow;
  const page = await pull("legacy", "6000");
  assert.deepEqual(
    page.ops.map((o) => o.op_id),
    ["new"],
  );
  assert.match(page.cursor, /^s:\d+$/);
  assert.deepEqual((await pull("legacy", page.cursor)).ops, []);
});

// Blobs and ops share one cursor: a blob stored after many ops is not skipped.
test("a blob uploaded after ops is delivered past their cursor", async () => {
  await push("mixed", [op({ op_id: "m1" }), op({ op_id: "m2" })]);
  const { cursor } = await pullAll("mixed");
  const res = await worker.fetch(
    new Request("https://sync.test/sync/blob/h1?vault_id=mixed&path=attachments%2Fa.png", {
      method: "POST",
      body: new Uint8Array([1, 2, 3]),
    }),
    env,
  );
  assert.equal(res.status, 201);
  const page = await pull("mixed", cursor);
  assert.deepEqual(
    page.blobs.map((b) => b.path),
    ["attachments/a.png"],
  );
});

// #10: an invalid vault id must not silently land in the "default" vault.
test("an invalid vault id is refused instead of mixed into default", async () => {
  const res = await push("Mi Vault", [op({ op_id: "wrong-vault" })]);
  assert.equal(res.status, 400);
  const def = await pullAll("default");
  assert.ok(!def.ops.some((o) => o.op_id === "wrong-vault"));
});

// #7: the trash retention must use the server's clock, not the device's.
test("a delete stamped by a device with a wrong clock does not erase history early", async () => {
  at(realNow());
  await push("clock", [op({ op_id: "content", note_id: "nc" })]);
  // Device clock set to 1970: its HLC looks decades old.
  await push("clock", [
    op({ op_id: "del", note_id: "nc", kind: "delete", content: null, hlc: { ts: 1_000, cnt: 0 } }),
  ]);
  await worker.scheduled({}, env);
  const { ops } = await pullAll("clock");
  Date.now = realNow;
  assert.ok(
    ops.some((o) => o.op_id === "content" && o.content),
    "the note history was purged on the device's clock",
  );
});

// Purge erases content but keeps delivery order intact for later ops.
test("after a purge, later ops are still delivered", async () => {
  await push("purge", [op({ op_id: "p1", note_id: "np" })]);
  const { cursor } = await pullAll("purge");
  await push("purge", [
    op({ op_id: "p2", note_id: "np", kind: "delete", content: null, purge: true }),
  ]);
  await push("purge", [op({ op_id: "p3", note_id: "other" })]);
  const later = await pullAll("purge", cursor);
  assert.deepEqual(
    later.ops.map((o) => o.op_id),
    ["p2", "p3"],
  );
  const all = await pullAll("purge");
  assert.ok(!all.ops.some((o) => o.note_id === "np" && o.content), "purged content still served");
});
