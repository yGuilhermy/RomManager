import test from "node:test";
import assert from "node:assert/strict";
import {
  filterConsoleGames,
  profileHint,
  unconfirmedInventory,
  consoleTargetName,
} from "../src/lib/console.ts";

test("destination labels show the selected folder before a long parent path", () => {
  const volume = { mount: "E:\\", label: "USB" };
  assert.equal(
    consoleTargetName({
      path: "\\\\?\\E:\\Users\\Name\\Collections\\PS2",
      volume,
    }),
    "Collections\\PS2 · E:\\ · USB",
  );
  assert.equal(
    consoleTargetName({ path: "\\\\?\\E:\\", volume }),
    "E:\\ · USB",
  );
});

test("an in-memory snapshot does not confirm the connection or claim a fresh scan", () => {
  const snapshot = {
    scanned_at: "2026-11-09T15:00:00Z",
    games: [],
    issues: [],
  };
  const original = {
    target: { id: "usb" },
    online: true,
    cached: false,
    connection_error: null,
    warnings: [],
    snapshot,
  };
  const stored = unconfirmedInventory(original);
  assert.equal(stored.online, false);
  assert.equal(stored.cached, true);
  assert.equal(stored.snapshot, snapshot);
  assert.equal(original.online, true);
  assert.equal(original.cached, false);
});

test("console search preserves Japanese and other non-Latin game titles", () => {
  const titles = [
    {
      id: "j",
      title: "ドラゴンクエスト",
      serial: null,
      method: "iso",
      issues: [],
    },
    { id: "l", title: "Dragon Quest", serial: null, method: "iso", issues: [] },
  ];
  assert.deepEqual(
    filterConsoleGames(titles, "ドラゴン", "all", false).map((g) => g.id),
    ["j"],
  );
});

const games = [
  {
    id: "a",
    title: "Original",
    serial: "SLUS-21355",
    method: "ul",
    issues: [],
  },
  {
    id: "b",
    title: "Mod edição",
    serial: "SLUS-21355",
    method: "iso",
    issues: ["Part missing"],
  },
];
test("console filtering preserves same-serial mods and combines filters", () => {
  assert.equal(
    filterConsoleGames(games, "slus_213.55", "all", false).length,
    2,
  );
  assert.deepEqual(
    filterConsoleGames(games, "edicao", "iso", true).map((g) => g.id),
    ["b"],
  );
  assert.equal(filterConsoleGames(games, "", "ul", true).length, 0);
});
test("profile guidance explains filesystem without claiming automatic OPL support", () => {
  assert.match(profileHint("legacy_fat32"), /FAT32/);
  assert.match(profileHint("modern_exfat"), /OPL.*exFAT/);
  assert.match(profileHint("unknown"), /FAT32/);
});
