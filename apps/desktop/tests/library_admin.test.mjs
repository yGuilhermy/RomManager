import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const appPath = path.resolve(__dirname, "../src/App.tsx");
const appContent = fs.readFileSync(appPath, "utf-8");
const cardPath = path.resolve(__dirname, "../src/components/LibraryGameCard.tsx");
const cardContent = fs.readFileSync(cardPath, "utf-8");

test("library card displays missing badge and multidisc label", () => {
    assert.ok(cardContent.includes("is_missing"), "LibraryGameCard should check is_missing");
    assert.ok(cardContent.includes("card-badge-danger"), "LibraryGameCard should render missing badge");
    assert.ok(cardContent.includes("Disco ${discMatch[1]}"), "LibraryGameCard should detect multidisc numbers");
    assert.ok(cardContent.includes("disabled={busy || isMissing}"), "LibraryGameCard should disable launch if missing");
});

test("library view supports missing banner and status filter", () => {
    assert.ok(appContent.includes("libraryFilterStatus"), "App should manage libraryFilterStatus state");
    assert.ok(appContent.includes("library-missing-banner"), "App should render library missing banner");
    assert.ok(appContent.includes("Ver ausentes"), "Banner should provide quick filter to missing ROMs");
});

test("game detail page integrates physical integrity check and missing indicators", () => {
    assert.ok(appContent.includes("checkIntegrity"), "App should define checkIntegrity method");
    assert.ok(appContent.includes("/verify"), "App should invoke verify endpoint");
    assert.ok(appContent.includes("integrity-panel"), "App should display integrity result panel");
    assert.ok(appContent.includes("Arquivo ausente no disco"), "App should disable launch button if missing on disk");
});

