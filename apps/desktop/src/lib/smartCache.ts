interface Entry {
    data?: unknown;
    hasData: boolean;
    at: number;
    promise?: Promise<unknown>;
}

const store = new Map<string, Entry>();
const listeners = new Set<(key: string) => void>();
const MAX_ENTRIES = 300;
export const DEFAULT_TTL = 30000;

export function peek<T>(key: string): T | undefined {
    const entry = store.get(key);
    return entry?.hasData ? (entry.data as T) : undefined;
}

export function subscribe(listener: (key: string) => void): () => void {
    listeners.add(listener);
    return () => { listeners.delete(listener); };
}

function notify(key: string) {
    for (const listener of [...listeners]) listener(key);
}

function trim() {
    while (store.size > MAX_ENTRIES) {
        const oldest = store.keys().next().value;
        if (oldest === undefined) break;
        store.delete(oldest);
    }
}

export function invalidate(prefix?: string) {
    if (!prefix) { store.clear(); return; }
    for (const key of [...store.keys()]) if (key.startsWith(prefix)) store.delete(key);
}

function start<T>(key: string, fetcher: () => Promise<T>, previous?: Entry): Promise<T> {
    const entry: Entry = { hasData: previous?.hasData ?? false, data: previous?.data, at: previous?.at ?? 0 };
    const promise = fetcher().then(
        data => {
            if (store.get(key) === entry) {
                const changed = !entry.hasData || JSON.stringify(entry.data) !== JSON.stringify(data);
                entry.data = changed ? data : entry.data;
                entry.hasData = true;
                entry.at = Date.now();
                entry.promise = undefined;
                if (changed) notify(key);
            }
            return (store.get(key) === entry ? entry.data : data) as T;
        },
        error => {
            if (store.get(key) === entry) {
                entry.promise = undefined;
                if (!entry.hasData) store.delete(key);
            }
            throw error;
        }
    );
    entry.promise = promise;
    store.delete(key);
    store.set(key, entry);
    trim();
    return promise;
}

export function loadCached<T>(key: string, fetcher: () => Promise<T>, ttl = DEFAULT_TTL): Promise<T> {
    const entry = store.get(key);
    if (!entry) return start(key, fetcher);
    if (entry.promise) return entry.hasData ? Promise.resolve(entry.data as T) : (entry.promise as Promise<T>);
    if (entry.hasData && Date.now() - entry.at < ttl) return Promise.resolve(entry.data as T);
    if (entry.hasData) {
        void start(key, fetcher, entry).catch(() => undefined);
        return Promise.resolve(entry.data as T);
    }
    return start(key, fetcher);
}

export function refreshCached<T>(key: string, fetcher: () => Promise<T>): Promise<T> {
    store.delete(key);
    return start(key, fetcher);
}

const queued = new Set<string>();
const pending: Array<{ key: string; run: () => Promise<unknown> }> = [];
let active = 0;
let paused = false;
const CONCURRENCY = 2;

function schedule(task: () => void) {
    const idle = (globalThis as { requestIdleCallback?: (cb: () => void, options?: { timeout: number }) => number }).requestIdleCallback;
    if (idle) idle(task, { timeout: 1500 });
    else setTimeout(task, 0);
}

function pump() {
    if (paused) return;
    while (active < CONCURRENCY && pending.length) {
        const next = pending.shift()!;
        active++;
        schedule(() => {
            next.run().catch(() => undefined).finally(() => {
                active--;
                queued.delete(next.key);
                setTimeout(pump, 40);
            });
        });
    }
}

export function prefetch<T>(key: string, fetcher: () => Promise<T>, ttl = DEFAULT_TTL) {
    const entry = store.get(key);
    if (queued.has(key) || entry?.promise || (entry?.hasData && Date.now() - entry.at < ttl)) return;
    queued.add(key);
    pending.push({ key, run: () => loadCached(key, fetcher, ttl) });
    pump();
}

export function prefetchTask(key: string, run: () => Promise<unknown>) {
    if (queued.has(key)) return;
    queued.add(key);
    pending.push({ key, run });
    pump();
}

export function pausePrefetch(value: boolean) {
    paused = value;
    if (!paused) pump();
}

export function prefetchPending() {
    return pending.length + active;
}
