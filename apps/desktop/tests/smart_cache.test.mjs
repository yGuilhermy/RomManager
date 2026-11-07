import { test } from "node:test";
import assert from "node:assert/strict";
import { loadCached, refreshCached, peek, invalidate, subscribe, prefetch } from "../src/lib/smartCache.ts";

const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));

test("concurrent reads share one request and later reads reuse the stored value", async () => {
    invalidate();
    let calls = 0;
    const fetcher = async () => ({ value: ++calls });
    const [a, b] = await Promise.all([loadCached("k", fetcher), loadCached("k", fetcher)]);
    assert.equal(a, b);
    assert.equal(calls, 1);
    assert.equal(await loadCached("k", fetcher), a);
    assert.equal(calls, 1);
    assert.equal(peek("k"), a);
});

test("stale values are returned immediately while a revalidation updates subscribers", async () => {
    invalidate();
    let version = 1;
    const fetcher = async () => ({ version });
    const first = await loadCached("s", fetcher, 5);
    await sleep(15);
    version = 2;
    const changes = [];
    const stop = subscribe(key => changes.push(key));
    const stale = await loadCached("s", fetcher, 5);
    assert.equal(stale, first);
    await sleep(15);
    stop();
    assert.deepEqual(peek("s"), { version: 2 });
    assert.deepEqual(changes, ["s"]);
});

test("unchanged revalidation keeps the same reference and does not notify", async () => {
    invalidate();
    const fetcher = async () => ({ same: true });
    const first = await loadCached("u", fetcher, 5);
    await sleep(15);
    const changes = [];
    const stop = subscribe(key => changes.push(key));
    await loadCached("u", fetcher, 5);
    await sleep(15);
    stop();
    assert.equal(peek("u"), first);
    assert.equal(changes.length, 0);
});

test("invalidation by prefix removes only matching entries and drops in-flight results", async () => {
    invalidate();
    await loadCached("/a/1", async () => 1);
    await loadCached("/b/1", async () => 2);
    invalidate("/a");
    assert.equal(peek("/a/1"), undefined);
    assert.equal(peek("/b/1"), 2);
    let release;
    const gate = new Promise(resolve => { release = resolve; });
    const inflight = loadCached("/a/2", async () => { await gate; return "old"; });
    invalidate("/a");
    release();
    await inflight;
    assert.equal(peek("/a/2"), undefined);
});

test("failed first loads are not cached and can be retried", async () => {
    invalidate();
    await assert.rejects(loadCached("f", async () => { throw new Error("down"); }), /down/);
    assert.equal(await loadCached("f", async () => "ok"), "ok");
});

test("forced refresh replaces cached data and prefetch warms the cache in the background", async () => {
    invalidate();
    let n = 0;
    await loadCached("r", async () => ++n);
    assert.equal(await refreshCached("r", async () => ++n), 2);
    prefetch("p", async () => "warm");
    for (let i = 0; i < 50 && peek("p") === undefined; i++) await sleep(20);
    assert.equal(peek("p"), "warm");
});
