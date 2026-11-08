import type { JobItem } from "../types";
type OnlineJob=Pick<JobItem,"id"|"kind"|"status">;
export function completedOnlineJobs(previous:Map<string,string>|null,jobs:OnlineJob[]):OnlineJob[]{
  if(previous===null)return [];
  return jobs.filter(job=>["extra","catalog-refresh"].includes(job.kind)&&["succeeded","failed","interrupted"].includes(job.status)&&previous.get(job.id)!==job.status);
}
export const EXTRAS_APPLIED_EVENT="rommanager:extras-applied";
export type ExtraKind = "saves" | "cheats" | "patches" | "textures";
export const extraLabels: Record<ExtraKind, string> = {
  saves: "Saves",
  cheats: "Cheats",
  patches: "Patches",
  textures: "Texturas",
};
export interface ExtraGame {
  cover_url?: string;
  key: string;
  title: string;
  serial: string | null;
  crcs: string[];
}
export interface ExtraItem {
  id: string;
  game_key: string;
  title: string;
  description: string;
  kind: ExtraKind;
  serial: string | null;
  crc: string | null;
  source: string;
  source_url: string;
  author: string;
  license: string;
  version: string;
  bytes: number;
  installable: boolean;
  install_note: string;
  groups: { name: string; description: string; automatic: boolean }[];
}
export interface GameResults {
  games: ExtraGame[];
  total: number;
  page: number;
  per_page?: number;
  total_pages?: number;
  stale: boolean;
  source: string;
  version: string;
}
export interface ItemResults {
  items: ExtraItem[];
  stale: boolean;
  source: string;
  version: string;
}
export interface ExtraReceipt {
  title: string;
  kind: ExtraKind;
  path: string;
  sha256: string;
  size: number;
  source_url: string;
  version: string;
  applied: boolean;
  message: string;
  plan_id: string | null;
  backup: string | null;
}
export interface ExtraOperation {
  job: JobItem;
  result: ExtraReceipt | null;
}
export interface ExtraRequest {
  save_confirmed?: boolean;
  action: "download" | "install";
  kind: ExtraKind;
  game_key: string;
  item_id: string;
  game_id?: string;
  crc?: string;
  installation_id?: string;
  card?: string;
  card_revision?: string;
  overwrite?: boolean;
  group_name?: string;
}
export function localTargets<T extends { serial: string | null }>(
  item: Pick<ExtraItem,"serial">,
  games: T[],
): T[] {
  return item.serial ? games.filter((game) => game.serial === item.serial) : [];
}
export function canApplyExtra(item:Pick<ExtraItem,"kind"|"serial"|"installable">, games:{serial:string|null}[]):boolean {
  return item.installable && (item.kind === "saves" || !!item.serial && localTargets(item,games).length > 0);
}
export function extraCover(game:Pick<ExtraGame,"title"|"serial"|"cover_url">):string{
  if(game.cover_url && /^https:\/\/raw\.githubusercontent\.com\/xlenore\/ps2-covers\/main\/covers\/default\/[A-Z]{4}-\d{5}\.jpg$/.test(game.cover_url))return game.cover_url;
  if(game.serial&&/^[A-Z]{4}-\d{5}$/.test(game.serial))return `https://raw.githubusercontent.com/xlenore/ps2-covers/main/covers/default/${game.serial}.jpg`;
  const title=game.title.replace(/[&:/\\*?"<>|]/g,"_").trim();
  return `https://cdn.jsdelivr.net/gh/libretro-thumbnails/Sony_-_PlayStation_2@master/Named_Boxarts/${encodeURIComponent(title)}%20(USA).png`;
}

const saveFormats = [
  { key: "max", name: "Max Drive (.max)", sourceName: "Max Drive" },
  { key: "zip", name: "PSV / ZIP (.zip)", sourceName: "PSV / ZIP" },
  { key: "psu", name: "PSU (.psu)", sourceName: "PSU" },
  { key: "cbs", name: "CodeBreaker (.cbs)", sourceName: "CodeBreaker" },
  { key: "sps", name: "SharkPort (.sps)", sourceName: "SharkPort" },
  { key: "xps", name: "X-Port (.xps)", sourceName: "X-Port" },
  { key: "xpo", name: "Xploder (.xpo)", sourceName: "Xploder" },
  { key: "other", name: "Outros formatos", sourceName: "" },
];
export interface SaveCategory {
  key: string;
  name: string;
  regions: { name: string; items: ExtraItem[] }[];
  items: ExtraItem[];
}
export function groupSaveItems(items: ExtraItem[]): SaveCategory[] {
  const categories = new Map<string, SaveCategory>();
  for (const item of items) {
    const parts = item.title.split(" · ");
    const gamefaqs = item.game_key.startsWith("GFQ:");
    const apollo = item.source_url === "https://github.com/bucanero/apollo-saves";
    const format = saveFormats.find(f => f.sourceName && gamefaqs && f.sourceName === parts[1])
      ?? saveFormats[apollo ? 2 : 7];
    const prefix = item.serial?.slice(0, 4);
    const region = gamefaqs && parts.length >= 3 ? parts.slice(2).join(" · ")
      : ["SLUS", "SCUS"].includes(prefix || "") ? "América do Norte"
      : ["SLES", "SCES"].includes(prefix || "") ? "Europa"
      : ["SLPS", "SLPM", "SCPS"].includes(prefix || "") ? "Japão"
      : "Região não informada";
    let category = categories.get(format.key);
    if (!category) {
      category = { key: format.key, name: format.name, regions: [], items: [] };
      categories.set(format.key, category);
    }
    category.items.push(item);
    let section = category.regions.find(r => r.name === region);
    if (!section) { section = { name: region, items: [] }; category.regions.push(section); }
    section.items.push(item);
  }
  return saveFormats.flatMap(format => {
    const category = categories.get(format.key);
    if (!category) return [];
    category.regions.sort((a, b) => a.name.localeCompare(b.name, "pt-BR"));
    return [category];
  });
}
