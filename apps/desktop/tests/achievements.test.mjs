import { test } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

test("AchievementsPage is wired to sidebar and game detail page in read-only consultation mode", () => {
    const appPath = path.resolve(__dirname, "../src/App.tsx");
    const appContent = fs.readFileSync(appPath, "utf-8");

    assert.ok(appContent.includes('import { AchievementsPage } from "./components/AchievementsPage";'));
    assert.ok(appContent.includes('{ id: "achievements" as Tab, label: "Conquistas" }'));
    assert.ok(appContent.includes('{activeTab === "achievements" && <AchievementsPage onShowToast={showToast} />}'));
    assert.ok(appContent.includes('detailSubTab === "achievements"'));
    assert.ok(appContent.includes('embeddedGameId={game.id}'));

    const pagePath = path.resolve(__dirname, "../src/components/AchievementsPage.tsx");
    const pageContent = fs.readFileSync(pagePath, "utf-8");

    assert.ok(pageContent.includes('htmlFor="ra-username"'));
    assert.ok(pageContent.includes('htmlFor="ra-secret"'));
    assert.ok(pageContent.includes('htmlFor="ra-game-search"'));
    assert.ok(pageContent.includes('htmlFor="ra-achievement-search"'));
    assert.ok(pageContent.includes("/api/pcsx2/achievements/state"));
    assert.ok(pageContent.includes("/api/pcsx2/achievements/login"));
    assert.ok(pageContent.includes("/api/pcsx2/achievements/logout"));
    assert.ok(pageContent.includes("/api/pcsx2/achievements/game/sync"));
    assert.ok(pageContent.includes("Desbloqueios ocorrem exclusivamente dentro do PCSX2 durante a emulação"));
    assert.ok(pageContent.includes("Não disponível em console PS2 físico via OPL"));
    assert.ok(pageContent.includes("tabular-nums"));
    assert.ok(!pageContent.includes("awardachievement"));
});

test("API mutationScope invalidates PCSX2 cache on achievements login, logout and sync", () => {
    const apiPath = path.resolve(__dirname, "../src/lib/api.ts");
    const apiContent = fs.readFileSync(apiPath, "utf-8");

    assert.ok(apiContent.includes('path.startsWith("/api/pcsx2/achievements")'));
});
