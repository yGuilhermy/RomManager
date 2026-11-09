export type ConsoleProfile = "unknown" | "legacy_fat32" | "modern_exfat";
export interface ConsoleVolume {
  identity: string;
  mount: string;
  label: string;
  filesystem: string;
  removable: boolean;
  free_bytes: number;
  total_bytes: number;
}
export interface ConsoleTarget {
  id: string;
  path: string;
  volume: ConsoleVolume;
  profile: ConsoleProfile;
}
export interface ConsoleGame {
  id: string;
  title: string;
  serial: string | null;
  method: string;
  media: string;
  files: string[];
  parts: number;
  found_parts: number;
  bytes: number;
  verification: string;
  issues: string[];
}
export interface ConsoleInventory {
  target: ConsoleTarget;
  online: boolean;
  cached: boolean;
  connection_error: string | null;
  warnings: string[];
  snapshot: {
    scanned_at: string;
    games: ConsoleGame[];
    issues: string[];
  } | null;
}
export function consoleDisplayPath(path: string): string {
  return path.replace(/^\\\\\?\\/, "");
}
export function consoleTargetName(target: ConsoleTarget): string {
  const path = consoleDisplayPath(target.path)
    .replace(/\//g, "\\")
    .replace(/\\+$/, "");
  const mount = consoleDisplayPath(target.volume.mount).replace(/\//g, "\\");
  const atRoot = path.toLowerCase() === mount.replace(/\\+$/, "").toLowerCase();
  const relative = atRoot
    ? ""
    : path.toLowerCase().startsWith(mount.toLowerCase())
      ? path.slice(mount.length)
      : path;
  const folder = relative.split("\\").filter(Boolean).slice(-2).join("\\");
  return [folder, mount, target.volume.label].filter(Boolean).join(" · ");
}
const fold = (value: string) =>
  value
    .normalize("NFD")
    .replace(/[\u0300-\u036f]/g, "")
    .toLowerCase()
    .replace(/[^\p{L}\p{N}]/gu, "");

export function unconfirmedInventory(view: ConsoleInventory): ConsoleInventory {
  return { ...view, cached: true, online: false, connection_error: null };
}
export function filterConsoleGames<
  T extends Pick<ConsoleGame, "title" | "serial" | "method" | "issues">,
>(games: T[], query: string, method: string, issuesOnly: boolean): T[] {
  const search = fold(query);
  return games.filter(
    (game) =>
      (!search ||
        fold(game.title).includes(search) ||
        fold(game.serial || "").includes(search)) &&
      (method === "all" || game.method === method) &&
      (!issuesOnly || game.issues.length > 0),
  );
}
export function profileHint(profile: ConsoleProfile): string {
  if (profile === "legacy_fat32")
    return "Pendrive FAT32 para OPL antigo ou moderno. Jogos grandes precisam ser divididos em UL.";
  if (profile === "modern_exfat")
    return "ISO direta em exFAT exige uma versão do OPL com suporte a exFAT. O formato do pendrive sozinho não confirma esse suporte.";
  return "Não sabe a versão? FAT32 com UL é a opção conservadora. Confirme o perfil antes de instalar; o app não formata o pendrive.";
}
