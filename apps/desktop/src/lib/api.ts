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
            const timeout = AbortSignal.timeout(path === "/api/pcsx2/install" ? 300000 : path.startsWith("/api/pcsx2/memory-cards") || path === "/api/pcsx2/resources" && method !== "GET" ? 180000 : path.includes("translations") ? 90000 : 30000);
            const signal = options?.signal ? AbortSignal.any([options.signal, timeout]) : timeout;
            const response = await fetch(url, { ...options, signal });
            if (!response.ok) {
                const body = await response.text();
                let message = body || `Erro HTTP ${response.status}`;
                try { const data = JSON.parse(body); message = data.error || data.message || message; } catch {}
                throw new ApiError(response.status, message);
            }
            if (method !== "GET" && (path.includes("/api/pcsx2/") || path.includes("/api/config"))) invalidatePcsx2Cache();
            return response;
        } catch (error) {
            if (options?.signal?.aborted) throw error;
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

const pcsx2Reads = new Map<string, Promise<unknown>>();
export function invalidatePcsx2Cache() { pcsx2Reads.clear(); }
export function cachedPcsx2Json<T>(path: string): Promise<T> {
    if (path.includes("refresh=true")) invalidatePcsx2Cache();
    let request = pcsx2Reads.get(path);
    if (!request) {
        request = apiJson<T>(path).catch(error => { if (pcsx2Reads.get(path) === request) pcsx2Reads.delete(path); throw error; });
        pcsx2Reads.set(path, request);
    }
    return request as Promise<T>;
}
