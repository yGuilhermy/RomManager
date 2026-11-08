import { invalidate, loadCached, peek, prefetch } from "./smartCache";

export const API = "http://127.0.0.1:3000";

export class ApiError extends Error {
    constructor(public status: number, message: string) { super(message); }
}

export async function apiFetch(path: string, options?: RequestInit): Promise<Response> {
    const url = path.startsWith("http") ? path : `${API}${path}`;
    const method = (options?.method || "GET").toUpperCase();
    const attempts = method === "GET" ? 3 : 1;
    for (let attempt = 0; attempt < attempts; attempt++) {
        try {
            const timeout = AbortSignal.timeout(path === "/api/pcsx2/install" ? 300000 : path.startsWith("/api/extras/games?") || path.startsWith("/api/extras/items?") || path.startsWith("/api/pcsx2/memory-cards") || path === "/api/pcsx2/resources" && method !== "GET" ? 180000 : path.includes("translations") ? 90000 : 30000);
            const signal = options?.signal ? AbortSignal.any([options.signal, timeout]) : timeout;
            const response = await fetch(url, { ...options, signal });
            if (!response.ok) {
                const body = await response.text();
                let message = body || `Erro HTTP ${response.status}`;
                try { const data = JSON.parse(body); message = data.error || data.message || message; } catch {}
                throw new ApiError(response.status, message);
            }
            if (method !== "GET" && (path.includes("/api/pcsx2/") || path.includes("/api/config"))) invalidatePcsx2Cache(path.includes("/api/pcsx2/") ? mutationScope(path.slice(path.indexOf("/api/pcsx2/"))) : undefined);
            return response;
        } catch (error) {
            if (options?.signal?.aborted) throw error;
            if (error instanceof Error && error.name === "TimeoutError" && path.startsWith("/api/extras/")) throw new Error("A fonte demorou para responder. Tente novamente; os dados salvos foram mantidos.");
            const retryable = error instanceof TypeError || error instanceof ApiError && [502, 503, 504].includes(error.status);
            if (!retryable || attempt === attempts - 1) throw error;
            await new Promise(resolve => setTimeout(resolve, 500 * (attempt + 1)));
        }
    }
    throw new Error("Não foi possível acessar o servidor");
}

export async function apiJson<T = unknown>(path: string, options?: RequestInit): Promise<T> {
    const response = await apiFetch(path, options);
    const data: unknown = await response.json();
    const endpoint = path.split("?")[0];
    if (["/api/games", "/api/downloads", "/api/favorites", "/api/history", "/api/logs", "/api/roms/translations"].includes(endpoint) && !Array.isArray(data)) {
        throw new Error("Resposta inválida do servidor: lista esperada");
    }
    if (endpoint === "/api/catalog" && (!data || typeof data !== "object" || !Array.isArray((data as {items?: unknown}).items))) {
        throw new Error("Resposta inválida do catálogo");
    }
    return data as T;
}

export function invalidatePcsx2Cache(prefix?: string) { invalidate(prefix); }
export function cachedPcsx2Json<T>(path: string): Promise<T> {
    if (path.includes("refresh=true")) invalidatePcsx2Cache();
    return loadCached<T>(path, () => apiJson<T>(path));
}
export function peekPcsx2Json<T>(path: string): T | undefined { return peek<T>(path); }
export function prefetchPcsx2Json(path: string) { prefetch(path, () => apiJson(path)); }

export function mutationScope(path: string): string | undefined {
    if (path.startsWith("/api/pcsx2/memory-cards")) return "/api/pcsx2/memory-cards";
    if (path.startsWith("/api/pcsx2/resources")) return "/api/pcsx2/resources";
    if (path.startsWith("/api/pcsx2/bios")) return "/api/pcsx2/bios";
    if (path.startsWith("/api/pcsx2/covers")) return "/api/pcsx2/resources";
    if (path.startsWith("/api/pcsx2/achievements")) return "/api/pcsx2/achievements";
    return undefined;
}
