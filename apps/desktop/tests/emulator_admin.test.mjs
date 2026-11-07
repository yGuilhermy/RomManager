import { test } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

test("bios page provides duplicate detection, legal compliance notice and active selection", () => {
    const biosPagePath = path.resolve(__dirname, "../src/components/BiosPage.tsx");
    const content = fs.readFileSync(biosPagePath, "utf-8");

    assert.ok(content.includes('/api/pcsx2/bios'));
    assert.ok(content.includes('/api/pcsx2/bios/import'));
    assert.ok(content.includes('/api/pcsx2/bios/select'));
    assert.ok(content.includes('não baixa BIOS'));
    assert.ok(content.includes('Duplicada'));
    assert.ok(content.includes('Usar BIOS'));
    assert.ok(content.includes('Importar BIOS'));
});

test("forms sanitize surrounding quotes and whitespace from paths", () => {
    const biosPagePath = path.resolve(__dirname, "../src/components/BiosPage.tsx");
    const biosContent = fs.readFileSync(biosPagePath, "utf-8");
    assert.ok(biosContent.includes('importPath.trim().replace(/^["\']|["\']$/g, "").trim()'));

});

test("bios page supports zip archives and sorts active bios first without activate button", () => {
    const biosPagePath = path.resolve(__dirname, "../src/components/BiosPage.tsx");
    const content = fs.readFileSync(biosPagePath, "utf-8");

    assert.ok(content.includes('pacote <code>.zip</code>'));
    assert.ok(content.includes('!bios.is_active && ('));
    assert.ok(content.includes('Usar BIOS'));
    assert.ok(content.includes('Ativa'));
});

test("native installation replaces emulator profiles and separate resource navigation", () => {
    const app=fs.readFileSync(path.resolve(__dirname,"../src/App.tsx"),"utf8");
    assert.ok(!app.includes("EmulatorPage"));
    assert.ok(!app.includes("/api/pcsx2/profiles"));
    assert.ok(!app.includes('id: "resources" as Tab'));
    assert.ok(app.includes("<GameResourcesPanel"));
    for(const label of ["Capas","Saves","Cheats","Patches","Texturas"]) assert.ok(app.includes(label));
});
test("memory cards prioritize import, readable titles and dismissible tool card",()=>{
    const cards=fs.readFileSync(path.resolve(__dirname,"../src/components/MemoryCardsPage.tsx"),"utf8");
    assert.ok(cards.indexOf("Importar save · PSU ou MAX")<cards.indexOf('className="save-list"'));
    assert.ok(cards.includes("save.title"));
    assert.ok(cards.includes("MemoryCardToolCard dismissible"));
    const settings=fs.readFileSync(path.resolve(__dirname,"../src/components/SettingsPage.tsx"),"utf8");
    assert.ok(settings.includes("<MemoryCardToolCard />"));
    assert.ok(settings.includes('key:"pcsx2_data_dir"'));
});
test("game controls use the native resource API and keep save management inline",()=>{
    const source=fs.readFileSync(path.resolve(__dirname,"../src/components/GameResourcesPanel.tsx"),"utf8");
    const hook=fs.readFileSync(path.resolve(__dirname,"../src/components/useGameResources.ts"),"utf8");
    assert.ok(hook.includes("/api/pcsx2/resources/game"));
    assert.ok(source.includes("<MemoryCardsPage embedded"));
    assert.ok(source.includes('controls[kind]'));
    assert.ok(source.includes('resource-option-action'));
    assert.ok(!source.includes('Jogo da biblioteca'));
});
