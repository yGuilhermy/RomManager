export type ResourceKind = "covers" | "saves" | "cheats" | "patches" | "textures";
export type NativeGroup = {
  name:string; description:string; author:string; kind:string; source:string;
  bundled:boolean; enabled:boolean; selected:boolean; automatic:boolean;
};
export type NativeState = {
  groups:NativeGroup[]; available_crcs:string[]; warnings:string[]; revision:string;
  files:{kind:string;name:string;path:string}[]; cheats:boolean; patches:boolean;
  textures:boolean; crc:string;
};
export type ResourcePlan = {
  id:string; installation_id:string; game_id:string; title:string; serial:string; crc:string;
  kind:string; destination:string; source:string;
  files:{relative:string;size:number;previous:string|null}[];
  applied:boolean; undone:boolean; total_bytes:number;
};
export type ResourceContext = {plans:ResourcePlan[];installation_id:string;running:boolean};
