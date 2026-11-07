import { test, afterEach } from "node:test";
import assert from "node:assert/strict";
import { mockIPC, clearMocks } from "@tauri-apps/api/mocks";
import { choosePath, saveGeneratedFile } from "../src/lib/nativeFiles.ts";

globalThis.window = {};
afterEach(() => { clearMocks(); delete globalThis.isTauri; });
test("native selection keeps full paths and passes file filters", async () => {
  globalThis.isTauri = true;
  mockIPC((command, payload) => {
    assert.equal(command, "plugin:dialog|open");
    assert.equal(payload.options.defaultPath, "C:\\My Saves\\old.max");
    assert.deepEqual(payload.options.filters[0].extensions, ["psu", "max"]);
    assert.equal(payload.options.multiple, false);
    return "D:\\Saves pessoais\\game.psu";
  });
  assert.equal(await choosePath({ title: "Save", current: ' "C:\\My Saves\\old.max" ', extensions: ["psu", "max"] }), "D:\\Saves pessoais\\game.psu");
});
test("cancelling folder selection returns null without a write command", async () => {
  globalThis.isTauri = true;
  mockIPC((command, payload) => {
    assert.equal(command, "plugin:dialog|open");
    assert.equal(payload.options.directory, true);
    return null;
  });
  assert.equal(await choosePath({ title: "Folder", directory: true }), null);
});
test("save dialog cancellation is distinct from a successful destination", async () => {
  globalThis.isTauri = true;
  const request = { kind: "save", source: "C:\\exports\\save.psu" };
  mockIPC((command, payload) => { assert.equal(command, "save_generated_file"); assert.deepEqual(payload.request, request); return null; });
  assert.equal(await saveGeneratedFile(request), null);
  mockIPC(() => "D:\\chosen.psu");
  assert.equal(await saveGeneratedFile(request), "D:\\chosen.psu");
});
test("browser-only runs report missing native integration instead of a fake path", async () => {
  await assert.rejects(choosePath({ title: "Folder" }), /desktop/);
});
