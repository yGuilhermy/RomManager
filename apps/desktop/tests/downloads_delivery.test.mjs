import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const appPath = path.resolve(__dirname, "../src/App.tsx");
const appContent = fs.readFileSync(appPath, "utf-8");

test("downloads page offers opening completed download in library", () => {
    assert.ok(appContent.includes("Abrir na biblioteca"), "Downloads should offer Abrir na biblioteca action");
    assert.ok(appContent.includes("Tentar novamente"), "Downloads should offer Tentar novamente on error/cancel");
});

test("downloads page contains no console installation action", () => {
    assert.ok(!appContent.includes("Instalar no console"), "Downloads must not contain console install action");
    assert.ok(!appContent.includes("Enviar para console"), "Downloads must not contain send to console action");
    assert.ok(!appContent.includes("Preparar para console"), "Downloads must not contain prepare console action");
});

test("downloads page provides queue pause and resume controls", () => {
    assert.ok(appContent.includes("Pausar fila"), "Should have Pausar fila button");
    assert.ok(appContent.includes("Retomar fila"), "Should have Retomar fila button");
    assert.ok(appContent.includes("Cancelar todos pendentes"), "Should have Cancelar todos pendentes button");
});

