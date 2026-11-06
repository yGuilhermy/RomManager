import { test } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

test("navigation includes emulator installation, profiles and bios tabs", () => {
    const appPath = path.resolve(__dirname, "../src/App.tsx");
    const content = fs.readFileSync(appPath, "utf-8");

    assert.ok(content.includes('{ id: "library" as Tab, label: "Biblioteca" }'));
    assert.ok(content.includes('{ id: "emulator" as Tab, label: "Instalação e Perfis" }'));
    assert.ok(content.includes('{ id: "bios" as Tab, label: "BIOS" }'));
    assert.ok(content.includes('import { EmulatorPage } from "./components/EmulatorPage";'));
    assert.ok(content.includes('import { BiosPage } from "./components/BiosPage";'));
    assert.ok(content.includes('{activeTab === "emulator" && <EmulatorPage onShowToast={showToast} />}'));
    assert.ok(content.includes('{activeTab === "bios" && <BiosPage onShowToast={showToast} />}'));
});

test("emulator page supports installations, profiles and covers export with rollback safeguards", () => {
    const emuPagePath = path.resolve(__dirname, "../src/components/EmulatorPage.tsx");
    const content = fs.readFileSync(emuPagePath, "utf-8");

    assert.ok(content.includes('subTab === "installations"'));
    assert.ok(content.includes('subTab === "profiles"'));
    assert.ok(!content.includes('subTab === "covers"'));
    assert.ok(content.includes('/api/pcsx2/detect'));
    assert.ok(content.includes('/api/pcsx2/releases'));
    assert.ok(content.includes('/api/pcsx2/profiles'));
    assert.ok(content.includes('/api/pcsx2/diagnostic'));
    assert.ok(content.includes('/api/pcsx2/covers/export'));
    assert.ok(content.includes('Baixar e instalar'));
    assert.ok(content.includes('PCSX2 aberto · alterações bloqueadas'));
    assert.ok(content.includes('Substituir capas existentes'));
    assert.ok(content.includes('Pasta de dados (opcional)'));
});

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

test("emulator page supports direct subtab deep linking via URLSearchParams", () => {
    const emuPagePath = path.resolve(__dirname, "../src/components/EmulatorPage.tsx");
    const content = fs.readFileSync(emuPagePath, "utf-8");

    assert.ok(content.includes('new URLSearchParams(window.location.search).get("subtab")'));
});

test("forms sanitize surrounding quotes and whitespace from paths", () => {
    const biosPagePath = path.resolve(__dirname, "../src/components/BiosPage.tsx");
    const biosContent = fs.readFileSync(biosPagePath, "utf-8");
    assert.ok(biosContent.includes('importPath.trim().replace(/^["\']|["\']$/g, "").trim()'));

    const emuPagePath = path.resolve(__dirname, "../src/components/EmulatorPage.tsx");
    const emuContent = fs.readFileSync(emuPagePath, "utf-8");
    assert.ok(emuContent.includes('newProfileExe.trim().replace(/^["\']|["\']$/g, "").trim()'));
});

test("bios page supports zip archives and sorts active bios first without activate button", () => {
    const biosPagePath = path.resolve(__dirname, "../src/components/BiosPage.tsx");
    const content = fs.readFileSync(biosPagePath, "utf-8");

    assert.ok(content.includes('pacote <code>.zip</code>'));
    assert.ok(content.includes('!bios.is_active && ('));
    assert.ok(content.includes('Usar BIOS'));
    assert.ok(content.includes('Ativa'));
});

test("emulator and library views support quick version selection and multiple profile binding", () => {
    const emuPagePath = path.resolve(__dirname, "../src/components/EmulatorPage.tsx");
    const emuContent = fs.readFileSync(emuPagePath, "utf-8");
    assert.ok(emuContent.includes('/api/pcsx2/version/select'));
    assert.ok(emuContent.includes('Instalação'));
    assert.ok(emuContent.includes('Instalação ativa'));
    assert.ok(emuContent.includes('id="emulator-version-select"'));
    assert.ok(emuContent.includes('Selecionar instalação'));
    assert.ok(emuContent.includes('Atualizar versões'));
    assert.ok(emuContent.includes('/api/pcsx2/releases?refresh=true'));

    const appPath = path.resolve(__dirname, "../src/App.tsx");
    const appContent = fs.readFileSync(appPath, "utf-8");
    assert.ok(appContent.includes('Versão do Emulador:'));
    assert.ok(appContent.includes('/api/pcsx2/version/select'));
    assert.ok(appContent.includes('/api/pcsx2/profiles/'));
});

test("formatPcsx2Version normalizes version strings cleanly without duplicate prefixes", async () => {
    const typesPath = path.resolve(__dirname, "../src/types.ts");
    const content = fs.readFileSync(typesPath, "utf-8");
    assert.ok(content.includes('export function formatPcsx2Version'));

    const { formatPcsx2Version } = await import("../src/types.ts");
    assert.equal(formatPcsx2Version("PCSX2 v2.8.2.0"), "PCSX2 v2.8.2.0");
    assert.equal(formatPcsx2Version("2.8.2"), "PCSX2 v2.8.2");
    assert.equal(formatPcsx2Version("v2.8.2"), "PCSX2 v2.8.2");
    assert.equal(formatPcsx2Version("2.9.155-nightly"), "PCSX2 v2.9.155-nightly");
});
