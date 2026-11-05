import type { ProfileBackup } from "../types";
export const PROFILE_COLORS = ["blue", "green", "amber", "rose", "slate"] as const;
export const PROGRESS_LABELS = { planned:"Quero jogar", playing:"Jogando", completed:"ConcluÃ­do", paused:"Pausado" } as const;
const object = (value:unknown): value is Record<string,unknown> => !!value && typeof value === "object" && !Array.isArray(value);
const text = (value:unknown,max:number) => typeof value === "string" && value.length <= max;
export function parseProfileBackup(input:unknown): ProfileBackup {
    if (!object(input) || input.format !== "rommanager-profile" || input.version !== 1) throw new Error("Este arquivo nÃ£o Ã© um backup de perfil RomManager compatÃ­vel.");
    const p=input.profile;
    if (!object(p) || !text(p.name,60) || !(p.name as string).trim() || !text(p.bio,300) || !PROFILE_COLORS.includes(p.color as typeof PROFILE_COLORS[number]) || !text(p.created_at,80) || !Number.isFinite(Date.parse(p.created_at as string))) throw new Error("Dados do perfil invÃ¡lidos no backup.");
    if (!Array.isArray(input.games) || input.games.length>10000 || !Array.isArray(input.favorites) || input.favorites.length>10000 || !Array.isArray(input.history) || input.history.length>20000) throw new Error("Backup com listas invÃ¡lidas ou acima do limite.");
    for (const game of input.games) {
        if (!object(game) || !text(game.game_id,4096) || !game.game_id || !Object.prototype.hasOwnProperty.call(PROGRESS_LABELS, game.status as string) || !text(game.notes,2000) || !Number.isInteger(game.launches) || (game.launches as number)<0 || (game.launches as number)>4294967295 || !(game.last_played===null || typeof game.last_played==="string" && Number.isFinite(Date.parse(game.last_played)))) throw new Error("Progresso de jogo invÃ¡lido no backup.");
    }
    for (const favorite of input.favorites) {
        if (!object(favorite) || !text(favorite.id,4096) || !favorite.id || !text(favorite.title,1024) || !text(favorite.url,8192)) throw new Error("Favorito invÃ¡lido no backup.");
        if (favorite.url) { try { if (!["http:","https:"].includes(new URL(favorite.url as string).protocol)) throw new Error(); } catch { throw new Error("URL de favorito invÃ¡lida no backup."); } }
    }
    for (const event of input.history) {
        if (!object(event) || !text(event.game_name,1024) || !text(event.action,40) || typeof event.timestamp!=="string" || !Number.isFinite(Date.parse(event.timestamp))) throw new Error("HistÃ³rico invÃ¡lido no backup.");
    }
    return input as unknown as ProfileBackup;
}
