import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const appPath = path.resolve(__dirname, "../src/App.tsx");
const appContent = fs.readFileSync(appPath, "utf-8");

test("catalog has region selector and search has unified source and format filters", () => {
    assert.ok(appContent.includes("catalogRegion"), "App.tsx should manage catalogRegion state");
    assert.ok(appContent.includes("searchFormat"), "App.tsx should manage searchFormat state");
    assert.ok(appContent.includes("Todas as fontes (Unificado)"), "App.tsx should provide unified search option");
    assert.ok(appContent.includes("Todas as regiões"), "App.tsx should provide region options in catalog");
});

test("catalog and search cards render format and region badges", () => {
    assert.ok(appContent.includes("card-badge-format"), "Cards should render format badge");
    assert.ok(appContent.includes("card-badge-region"), "Cards should render region badge");
});

test("cards provide direct actions to open library when installed or downloads when in queue", () => {
    assert.ok(appContent.includes("Ver na biblioteca"), "Cards should offer Ver na biblioteca action");
    assert.ok(appContent.includes("Ver na fila"), "Cards should offer Ver na fila action");
});

