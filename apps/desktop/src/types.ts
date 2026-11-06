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

export interface Pcsx2Installation {
    path: string;
    version: string;
    is_portable: boolean;
    is_managed: boolean;
    data_dir: string;
    bios_dir: string;
    covers_dir: string;
    inis_dir: string;
    is_running: boolean;
}

export interface Pcsx2Profile {
    id: string;
    name: string;
    executable_path: string;
    data_dir: string;
    bios_path: string | null;
    renderer: string;
    aspect_ratio: string;
    fullscreen: boolean;
    is_active: boolean;
    created_at: string;
    updated_at: string;
}

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

export interface Pcsx2Diagnostic {
    executable_valid: boolean;
    executable_path: string;
    executable_version: string;
    executable_type: string;
    is_running: boolean;
    data_dir_valid: boolean;
    data_dir_writable: boolean;
    bios_dir_valid: boolean;
    bios_count: number;
    active_bios_valid: boolean;
    covers_dir_valid: boolean;
    roms_dir_valid: boolean;
    warnings: string[];
    errors: string[];
}

export interface ManagedRelease {
    version: string;
    release_name: string;
    channel: string;
    published_at: string;
    download_url: string;
    file_size: number;
    installed: boolean;
    executable_path?: string | null;
    data_dir?: string | null;
}

export interface CoversExportResult {
    total_games: number;
    exported: number;
    preserved: number;
    skipped_no_serial: number;
    errors: number;
    target_directory: string;
}

export function formatPcsx2Version(version?: string): string {
    if (!version) return "PCSX2";
    let v = version.trim();
    if (v.toLowerCase().startsWith("pcsx2")) {
        v = v.substring(5).trim();
    }
    if (!v.startsWith("v") && !v.startsWith("V") && /^\d/.test(v)) {
        v = `v${v}`;
    }
    return `PCSX2 ${v}`.trim();
}

