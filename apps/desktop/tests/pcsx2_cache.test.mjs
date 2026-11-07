import { test } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import { fileURLToPath } from "node:url";
import { buildSync } from "esbuild";
const js = buildSync({ entryPoints: [fileURLToPath(new URL("../src/lib/api.ts", import.meta.url))], bundle: true, format: "esm", target: "es2022", write: false }).outputFiles[0].text;
const { cachedPcsx2Json, invalidatePcsx2Cache, apiFetch } = await import(`data:text/javascript;base64,${Buffer.from(js).toString("base64")}`);

test("PCSX2 cache deduplicates reads and refreshes after mutation or explicit update", async () => {
    const original = globalThis.fetch;
    let reads = 0;
    globalThis.fetch = async (_url, options) => {
        if ((options?.method || "GET") !== "GET") return new Response("{}", { status: 200 });
        return new Response(JSON.stringify({ revision: ++reads }), { status: 200 });
    };
    try {
        invalidatePcsx2Cache();
        const [a, b] = await Promise.all([cachedPcsx2Json("/api/pcsx2/bios"), cachedPcsx2Json("/api/pcsx2/bios")]);
        assert.deepEqual(a, b);
        assert.equal(reads, 1);
        await cachedPcsx2Json("/api/pcsx2/bios");
        assert.equal(reads, 1);
        await apiFetch("/api/pcsx2/bios/select", { method: "POST" });
        assert.deepEqual(await cachedPcsx2Json("/api/pcsx2/bios"), { revision: 2 });
        invalidatePcsx2Cache();
        await cachedPcsx2Json("/api/pcsx2/bios");
        assert.equal(reads, 3);
    } finally { globalThis.fetch = original; invalidatePcsx2Cache(); }
});

test("failed cached requests can be retried and failed mutations preserve cache", async () => {
    const original = globalThis.fetch;
    let failed = true;
    globalThis.fetch = async (_url, options) => new Response(failed || options?.method === "POST" ? "unavailable" : "[]", { status: failed || options?.method === "POST" ? 400 : 200 });
    try {
        invalidatePcsx2Cache();
        await assert.rejects(cachedPcsx2Json("/api/pcsx2/detect"));
        failed = false;
        const result = await cachedPcsx2Json("/api/pcsx2/detect");
        await assert.rejects(apiFetch("/api/pcsx2/profiles", { method: "POST" }));
        assert.equal(await cachedPcsx2Json("/api/pcsx2/detect"), result);
    } finally { globalThis.fetch = original; invalidatePcsx2Cache(); }
});
