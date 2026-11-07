import { test } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

test("navigation structure contains PlayStation 2, Emulador and Geral groups in correct hierarchy", () => {
    const appPath = path.resolve(__dirname, "../src/App.tsx");
    const content = fs.readFileSync(appPath, "utf-8");

    assert.ok(content.includes("PLAYSTATION 2"), "Missing PLAYSTATION 2 section label");
    assert.ok(content.includes("EMULADOR · PCSX2"), "Missing EMULADOR · PCSX2 section label");
    assert.ok(content.includes("GERAL"), "Missing GERAL section label");

    const ps2Items = ['{ id: "catalog" as Tab, label: "Catálogo PS2" }', '{ id: "search" as Tab, label: "Pesquisa" }', '{ id: "downloads" as Tab, label: "Downloads" }'];
    for (const item of ps2Items) {
        assert.ok(content.includes(item), `Missing PS2 item: ${item}`);
    }

    const emulatorItems = [
        '{ id: "library" as Tab, label: "Biblioteca" }',
        '{ id: "bios" as Tab, label: "BIOS" }',
        '{ id: "memory-cards" as Tab, label: "Memory cards" }',
        '{ id: "achievements" as Tab, label: "Conquistas" }',
    ];
    for (const item of emulatorItems) {
        assert.ok(content.includes(item), `Missing Emulator item: ${item}`);
    }

    const generalItems = ['{ id: "activity" as Tab, label: "Atividade" }', '{ id: "profile" as Tab, label: "Perfil" }', '{ id: "settings" as Tab, label: "Configurações" }'];
    for (const item of generalItems) {
        assert.ok(content.includes(item), `Missing General item: ${item}`);
    }

    assert.ok(!content.includes("CONSOLE · PS2"), "Console group should not appear in stage A");
    assert.ok(!content.includes("Dispositivos"), "Dispositivos should not appear in stage A");
    assert.ok(!content.includes("Transferência OPL"), "OPL transfer should not appear in stage A");
    assert.ok(!content.includes("Memória USB"), "USB transfer should not appear in stage A");
});

test("activity page component is correctly wired to activeTab", () => {
    const appPath = path.resolve(__dirname, "../src/App.tsx");
    const content = fs.readFileSync(appPath, "utf-8");

    assert.ok(content.includes('import { ActivityPage } from "./components/ActivityPage";'));
    assert.ok(content.includes('{activeTab === "activity" && <ActivityPage jobs={jobs} onRefresh={fetchJobs} />}'));
    assert.ok(content.includes('item.id === "activity" && activeJobsCount > 0'));
    assert.ok(content.includes('<span className="nav-badge">{activeJobsCount}</span>'));
});
