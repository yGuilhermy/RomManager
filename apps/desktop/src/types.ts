export interface InstalledGame { id:string; title:string; filename:string; file_path:string; file_size:number; format:string; cover_url:string|null; region:string|null; serial:string|null; date_added:string; is_missing?:boolean; }
export interface IntegrityResult { id:string; title:string; file_path:string; exists:boolean; actual_size:number; expected_size:number; size_matches:boolean; valid_format:boolean; format:string; serial:string|null; healthy:boolean; message:string; }
export interface FavoriteItem { id:string; title:string; cover_url:string|null; url:string; size:number|null; region:string|null; added_at:string|null; }
export interface HistoryItem { id:number; game_name:string; action:string; timestamp:string; details:string|null; }
export interface LocalProfile { name:string; bio:string; color:string; created_at:string; }
export type ProgressStatus = "planned" | "playing" | "completed" | "paused";
export interface GameProgress { game_id:string; status:ProgressStatus; notes:string; launches:number; last_played:string|null; }
export interface ProfileState { profile:LocalProfile; games:GameProgress[]; }
export interface ProfileBackup { format:"rommanager-profile"; version:1; profile:LocalProfile; games:GameProgress[]; favorites:FavoriteItem[]; history:HistoryItem[]; }
export interface CoverJob { running:boolean; total:number; processed:number; updated:number; preserved:number; failed:number; current:string; error:string|null; }
export interface ValidationResult { valid:boolean; message:string; }
export type JobStatus = "queued" | "running" | "cancel_requested" | "succeeded" | "failed" | "interrupted";
export interface JobItem { id:string; kind:string; title:string; status:JobStatus; progress:number; current_step:string; total_steps:number; completed_steps:number; error_message:string|null; created_at:string; updated_at:string; finished_at:string|null; }

export interface BiosInfo {
    filename: string;
    path: string;
    size: number;
    sha1: string;
    region: string;
    version: string;
    model: string;
    is_valid: boolean;
    is_active: boolean;
    is_duplicate: boolean;
}

export interface BiosImportResult {
    count: number;
    items: BiosInfo[];
    message: string;
    filename: string;
    path: string;
    size: number;
    sha1: string;
    region: string;
    version: string;
    model: string;
    is_valid: boolean;
    is_active: boolean;
    is_duplicate: boolean;
}

export interface Pcsx2AchievementsConfig {
    enabled: boolean;
    hardcore_mode: boolean;
    encore_mode: boolean;
    spectator_mode: boolean;
    notifications: boolean;
    sound_effects: boolean;
    overlays: boolean;
    pcsx2_username: string | null;
}

export interface RaAccountSummary {
    username: string;
    score: number;
    softcore_score: number;
    avatar_url: string;
    profile_url: string;
    synced_at: string;
}

export interface RaAchievementItem {
    id: number;
    title: string;
    description: string;
    points: number;
    badge_name: string;
    badge_url: string;
    badge_locked_url: string;
    kind: string | null;
    unlocked_casual: boolean;
    unlocked_hardcore: boolean;
    official_url: string;
}

export interface RaGameProgress {
    game_id: string;
    game_title: string;
    serial: string | null;
    format: string;
    file_hash: string;
    boot_executable: string;
    ra_game_id: number;
    ra_title: string;
    icon_url: string;
    official_game_url: string | null;
    recognition_status: string;
    recognition_message: string;
    total_achievements: number;
    unlocked_casual: number;
    unlocked_hardcore: number;
    points_total: number;
    points_earned_casual: number;
    points_earned_hardcore: number;
    achievements: RaAchievementItem[];
    synced_at: string | null;
    from_cache: boolean;
    offline: boolean;
}

export interface RaGameSummaryRow {
    game_id: string;
    game_title: string;
    serial: string | null;
    format: string;
    ra_game_id: number;
    ra_title: string;
    icon_url: string;
    official_game_url: string | null;
    recognition_status: string;
    recognition_message: string;
    total_achievements: number;
    unlocked_casual: number;
    unlocked_hardcore: number;
    points_total: number;
    points_earned_casual: number;
    points_earned_hardcore: number;
    synced_at: string | null;
}

export interface AchievementsState {
    linked: boolean;
    account: RaAccountSummary | null;
    pcsx2: Pcsx2AchievementsConfig;
    games: RaGameSummaryRow[];
    offline: boolean;
    notice: string | null;
    syncing?: boolean;
    sync_completed?: number;
    sync_total?: number;
    sync_current_game?: string | null;
    cache_revision?: number;
}
