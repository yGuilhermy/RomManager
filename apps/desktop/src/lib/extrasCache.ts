import { loadCached, peek, refreshCached } from "./smartCache";

const STORAGE = "rommanager:extras:v1";
const MAX_ENTRIES = 80;
const MAX_BYTES = 3_000_000;
type Entry = { at: number; data: unknown };
const saved = new Map<string, Entry>();
let restored = false;

function restore() {
  if (restored) return;
  restored = true;
  try {
    const raw = sessionStorage.getItem(STORAGE);
    if (!raw || raw.length > MAX_BYTES) return;
    const entries: unknown = JSON.parse(raw);
    if (!Array.isArray(entries)) return;
    for (const row of entries.slice(-MAX_ENTRIES)) {
      if (!Array.isArray(row) || typeof row[0] !== "string" || !row[0].startsWith("/api/extras/")) continue;
      const entry = row[1] as Entry | undefined;
      if (entry && Number.isFinite(entry.at) && valid(entry.data)) saved.set(row[0], entry);
    }
  } catch { /* A disabled or full session store does not block browsing. */ }
}
function valid(data: unknown) {
  if (!data || typeof data !== "object") return false;
  const value = data as { games?: unknown; items?: unknown; version?: unknown };
  return typeof value.version === "string" && (Array.isArray(value.games) || Array.isArray(value.items));
}
function ttl(path: string) {
  return path.includes("source=gamefaqs") || /key=GFQ(?:%3A|:)/i.test(path) ? Infinity : 300000;
}
function remember(path: string, data: unknown) {
  if (!valid(data)) return;
  restore();
  saved.delete(path);
  saved.set(path, { at: Date.now(), data });
  while (saved.size > MAX_ENTRIES) saved.delete(saved.keys().next().value!);
  try {
    let raw = JSON.stringify([...saved]);
    while (raw.length > MAX_BYTES && saved.size) {
      saved.delete(saved.keys().next().value!);
      raw = JSON.stringify([...saved]);
    }
    sessionStorage.setItem(STORAGE, raw);
  } catch { /* Memory cache remains usable when persistence is unavailable. */ }
}
export function peekExtras<T>(path: string): T | undefined {
  restore();
  return peek<T>(path) ?? saved.get(path)?.data as T | undefined;
}
export async function loadExtras<T>(path: string, fetcher: () => Promise<T>, force = false): Promise<T> {
  restore();
  const entry = saved.get(path);
  const request = async () => {
    const data = await fetcher();
    remember(path, data);
    return data;
  };
  const data = await (force ? refreshCached(path, request) : loadCached(path,
    entry && Date.now() - entry.at < ttl(path) ? async () => entry.data as T : request, ttl(path)));
  return data;
}
