import { apiFetch, cachedPcsx2Json, prefetchPcsx2Json } from "./api";
import { prefetchTask } from "./smartCache";

type CardList = { cards: { name: string; kind: string; slots: number[] }[] };

export const CARDS_SUMMARY_PATH = "/api/pcsx2/memory-cards?summary=true";
export const RESOURCES_PATH = "/api/pcsx2/resources";
export const cardPath = (name: string) => `/api/pcsx2/memory-cards?card=${encodeURIComponent(name)}`;
export const gameStatePath = (gameId: string, crc = "") => `/api/pcsx2/resources/game?game_id=${encodeURIComponent(gameId)}&crc=${encodeURIComponent(crc)}`;

const warmedGames = new Map<string, number>();
const GAME_WARM_INTERVAL = 60000;
let warmedApp = false;

export function defaultCardName(cards: CardList["cards"]): string {
    return cards.find(card => card.slots.includes(1))?.name || cards.find(card => card.kind === "file")?.name || cards[0]?.name || "";
}

export function prefetchAppWarmup() {
    if (warmedApp) return;
    warmedApp = true;
    prefetchPcsx2Json(RESOURCES_PATH);
    prefetchTask("warmup:achievements", () => apiFetch("/api/pcsx2/achievements/state"));
    prefetchTask("warmup:cards", async () => {
        const state = await cachedPcsx2Json<CardList>(CARDS_SUMMARY_PATH);
        const name = defaultCardName(state.cards);
        if (name) prefetchPcsx2Json(cardPath(name));
    });
}

export function prefetchGameBundle(game: { id: string; title: string; serial?: string | null }) {
    const last = warmedGames.get(game.id) || 0;
    if (Date.now() - last < GAME_WARM_INTERVAL) return;
    warmedGames.set(game.id, Date.now());
    prefetchPcsx2Json(RESOURCES_PATH);
    prefetchPcsx2Json(CARDS_SUMMARY_PATH);
    if (game.serial) prefetchPcsx2Json(gameStatePath(game.id));
    prefetchTask(`warm:${game.id}`, () => apiFetch("/api/cache/prefetch", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ game_id: game.id, title: game.title, serial: game.serial || null })
    }));
}

export function resetPrefetchState() {
    warmedGames.clear();
    warmedApp = false;
}
