export interface InstalledGame { id:string; title:string; filename:string; file_path:string; file_size:number; format:string; cover_url:string|null; region:string|null; serial:string|null; date_added:string; }
export interface FavoriteItem { id:string; title:string; cover_url:string|null; url:string; size:number|null; region:string|null; added_at:string|null; }
export interface HistoryItem { id:number; game_name:string; action:string; timestamp:string; details:string|null; }
export interface LocalProfile { name:string; bio:string; color:string; created_at:string; }
export type ProgressStatus = "planned" | "playing" | "completed" | "paused";
export interface GameProgress { game_id:string; status:ProgressStatus; notes:string; launches:number; last_played:string|null; }
export interface ProfileState { profile:LocalProfile; games:GameProgress[]; }
export interface ProfileBackup { format:"rommanager-profile"; version:1; profile:LocalProfile; games:GameProgress[]; favorites:FavoriteItem[]; history:HistoryItem[]; }
export interface CoverJob { running:boolean; total:number; processed:number; updated:number; preserved:number; failed:number; current:string; error:string|null; }
export interface ValidationResult { valid:boolean; message:string; }
