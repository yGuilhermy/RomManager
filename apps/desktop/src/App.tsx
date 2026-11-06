import { useState, useEffect, useCallback, useRef } from "react";
import "./App.css";
import { apiFetch, apiJson } from "./lib/api";
import { DeleteGameDialog } from "./components/DeleteGameDialog";
import type { InstalledGame, FavoriteItem, HistoryItem, ValidationResult, CoverJob, JobItem, IntegrityResult, Pcsx2Installation, Pcsx2Profile } from "./types";
import { formatPcsx2Version } from "./types";
import { formatBytes } from "./lib/format";
import { CoverImage } from "./components/CoverImage";
import { LibraryGameCard } from "./components/LibraryGameCard";
import { SettingsPage } from "./components/SettingsPage";
import { ProfilePage } from "./components/ProfilePage";
import { ActivityPage } from "./components/ActivityPage";
import { EmulatorPage } from "./components/EmulatorPage";
import { BiosPage } from "./components/BiosPage";
import packageInfo from "../package.json";
import logoUrl from "./assets/logo.png";
import { ToastContainer, showToast } from "./components/Toast";

interface RomCatalogItem {
    id: string;
    title: string;
    filename: string;
    part: string;
    size: number;
    region: string;
    url: string;
    cover_url: string;
    format?: string;
    source?: string;
}

interface CatalogResponse {
    items: RomCatalogItem[];
    total: number;
    page: number;
    per_page: number;
    total_pages: number;
}

interface DownloadItem {
    id: string;
    game_title: string;
    url: string;
    cover_url?: string | null;
    status: string;
    progress: number;
    speed: number;
    size: number;
    downloaded_bytes: number;
    error_message?: string | null;
    target_path?: string | null;
    connections?: number | null;
}

interface Pcsx2Compat {
    title: string;
    serial: string;
    crc?: string | null;
    region?: string | null;
    status: string;
    last_tested_version?: string | null;
    last_tested_date?: string | null;
    wiki_link?: string | null;
    forum_link?: string | null;
}

interface GameMetadata {
    title: string;
    description: string | null;
    source: string | null;
    compatibility: Pcsx2Compat | null;
}

function renderCompatBadge(status: string) {
    const s = status.toLowerCase();
    if (s.includes("perfect") || s.includes("playable")) {
        return (
            <span className="compat-badge-pill playable">
                ✓ {status} (Jogável Oficial)
            </span>
        );
    }
    if (s.includes("ingame")) {
        return (
            <span className="compat-badge-pill ingame">
                ⚠ {status} (Entra no jogo)
            </span>
        );
    }
    return (
        <span className="compat-badge-pill unplayable">
            ✕ {status}
        </span>
    );
}

type Tab = "library" | "emulator" | "bios" | "catalog" | "search" | "downloads" | "activity" | "settings" | "profile" | "game-page";

const PER_PAGE_OPTIONS = [12, 24, 48, 96];

const STATUS_LABEL: Record<string, string> = {
    Queued: "Na fila",
    queued: "Na fila",
    pending: "Na fila",
    Pending: "Na fila",
    Downloading: "Baixando",
    downloading: "Baixando",
    Extracting: "Extraindo",
    extracting: "Extraindo",
    Copying: "Instalando",
    copying: "Instalando",
    Completed: "Concluído",
    completed: "Concluído",
    Cancelled: "Cancelado",
    cancelled: "Cancelado",
    Error: "Erro",
    error: "Erro"
};

function renderNavIcon(id: Tab) {
    switch (id) {
        case "library":
            return (
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                    <path d="M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z" />
                    <polyline points="3.27 6.96 12 12.01 20.73 6.96" />
                    <line x1="12" y1="22.08" x2="12" y2="12" />
                </svg>
            );
        case "emulator":
            return (
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                    <rect x="2" y="6" width="20" height="12" rx="2" />
                    <line x1="6" y1="12" x2="10" y2="12" />
                    <line x1="8" y1="10" x2="8" y2="14" />
                    <line x1="15" y1="11" x2="15.01" y2="11" />
                    <line x1="18" y1="13" x2="18.01" y2="13" />
                </svg>
            );
        case "bios":
            return (
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                    <rect x="4" y="4" width="16" height="16" rx="2" />
                    <rect x="9" y="9" width="6" height="6" />
                    <line x1="9" y1="1" x2="9" y2="4" />
                    <line x1="15" y1="1" x2="15" y2="4" />
                    <line x1="9" y1="20" x2="9" y2="23" />
                    <line x1="15" y1="20" x2="15" y2="23" />
                    <line x1="20" y1="9" x2="23" y2="9" />
                    <line x1="20" y1="14" x2="23" y2="14" />
                    <line x1="1" y1="9" x2="4" y2="9" />
                    <line x1="1" y1="14" x2="4" y2="14" />
                </svg>
            );
        case "catalog":
            return (
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                    <circle cx="12" cy="12" r="10" />
                    <path d="M16 11.5c0-2.5-1.8-4.5-4-4.5s-4 2-4 4.5 1.8 4.5 4 4.5c1.8 0 3.2-1.2 3.8-3H12" />
                </svg>
            );
        case "search":
            return (
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                    <circle cx="11" cy="11" r="8" />
                    <line x1="21" y1="21" x2="16.65" y2="16.65" />
                </svg>
            );
        case "downloads":
            return (
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                    <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
                    <polyline points="7 10 12 15 17 10" />
                    <line x1="12" y1="15" x2="12" y2="3" />
                </svg>
            );
        case "activity":
            return (
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                    <polyline points="22 12 18 12 15 21 9 3 6 12 2 12" />
                </svg>
            );
        case "settings":
            return (
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                    <circle cx="12" cy="12" r="3" />
                    <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
                </svg>
            );
        case "profile":
            return (
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                    <path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2" />
                    <circle cx="12" cy="7" r="4" />
                </svg>
            );
        default:
            return null;
    }
}


export default function App() {
    const [activeTab, setActiveTab] = useState<Tab>(() => {
        try {
            const t = new URLSearchParams(window.location.search).get("tab") as Tab;
            if (t) return t;
        } catch {}
        return "catalog";
    });
    const [previousTab, setPreviousTab] = useState<Tab>("catalog");

    const [installedGames, setInstalledGames] = useState<InstalledGame[]>([]);
    const [librarySearch, setLibrarySearch] = useState("");
    const [librarySort, setLibrarySort] = useState("name_asc");
    const [libraryFilterRegion, setLibraryFilterRegion] = useState("all");
    const [libraryFilterFormat, setLibraryFilterFormat] = useState("all");
    const [libraryFilterStatus, setLibraryFilterStatus] = useState<"all" | "available" | "missing">("all");
    const [viewMode, setViewMode] = useState<"grid" | "list">(() => { try { return localStorage.getItem("rom_library_view") === "list" ? "list" : "grid"; } catch { return "grid"; } });
    const [isScanning, setIsScanning] = useState(false);
    const [coverJob, setCoverJob] = useState<CoverJob | null>(null);
    useEffect(() => { try { localStorage.setItem("rom_library_view", viewMode); } catch {} }, [viewMode]);
    const [gameToDelete, setGameToDelete] = useState<{ id: string; title: string; filePath?: string; coverUrl?: string } | null>(null);
    const [isDeletingGame, setIsDeletingGame] = useState<boolean>(false);

    const [catalog, setCatalog] = useState<CatalogResponse>({ items: [], total: 0, page: 1, per_page: 24, total_pages: 1 });
    const [catalogSearch, setCatalogSearch] = useState("");
    const [catalogTab, setCatalogTab] = useState<"popular" | "az">("popular");
    const [catalogLetter, setCatalogLetter] = useState("");
    const [catalogSort, setCatalogSort] = useState<"title_asc" | "title_desc">("title_asc");
    const [catalogRegion, setCatalogRegion] = useState("all");
    const [catalogPage, setCatalogPage] = useState(1);
    const [perPage, setPerPage] = useState(24);
    const [jumpPage, setJumpPage] = useState("");
    const [isCatalogLoading, setIsCatalogLoading] = useState(false);

    const [searchQuery, setSearchQuery] = useState(() => {
        try { return new URLSearchParams(window.location.search).get("q") || ""; } catch { return ""; }
    });
    const [searchGenre, setSearchGenre] = useState("");
    const [searchLang, setSearchLang] = useState("all");
    const [searchFormat, setSearchFormat] = useState("all");
    const [searchSource, setSearchSource] = useState("all");
    const [searchSort, setSearchSort] = useState("relevance");
    const [searchPerPage, setSearchPerPage] = useState(24);
    const [searchResults, setSearchResults] = useState<RomCatalogItem[]>([]);
    const [isSearching, setIsSearching] = useState(false);

    const [downloads, setDownloads] = useState<DownloadItem[]>([]);
    const [selectedBulk, setSelectedBulk] = useState<Map<string, RomCatalogItem>>(() => new Map());
    const [isBulkSelectActive, setIsBulkSelectActive] = useState(false);
    const [queuePaused, setQueuePaused] = useState(false);
    const [jobs, setJobs] = useState<JobItem[]>([]);

    const [configs, setConfigs] = useState<Record<string, string>>({
        roms_directory: "",
        pcsx2_executable_path: "C:\\Program Files\\PCSX2\\pcsx2-qt.exe",
        covers_directory: "",
        max_concurrent_downloads: "2"
    });
    const [isSavingConfig, setIsSavingConfig] = useState(false);
    const [romsValidation, setRomsValidation] = useState<ValidationResult | null>(null);
    const [pcsx2Validation, setPcsx2Validation] = useState<ValidationResult | null>(null);
    const [coversValidation, setCoversValidation] = useState<ValidationResult | null>(null);
    const [logs, setLogs] = useState<string[]>([]);

    const [showSetupWizard, setShowSetupWizard] = useState<boolean>(() => {
        try {
            if (window.location.search.includes("skip_wizard") || window.location.search.includes("tab=")) return false;
            return localStorage.getItem("setup_done") === "0";
        } catch {
            return false;
        }
    });
    const [wizardStep, setWizardStep] = useState<number>(0);
    const [wizardRomsDir, setWizardRomsDir] = useState<string>("");
    const [wizardPcsx2Path, setWizardPcsx2Path] = useState<string>("");
    const [wizardCoversDir, setWizardCoversDir] = useState<string>("");
    const [wizardAutoCovers, setWizardAutoCovers] = useState<boolean>(true);
    const [wizardMaxConcurrent, setWizardMaxConcurrent] = useState<string>("2");
    const [wizardIsFinishing, setWizardIsFinishing] = useState<boolean>(false);
    const [wizardRomsCheck, setWizardRomsCheck] = useState<ValidationResult | null>(null);
    const [wizardPcsx2Check, setWizardPcsx2Check] = useState<ValidationResult | null>(null);
    const [wizardCoversCheck, setWizardCoversCheck] = useState<ValidationResult | null>(null);

    const [favorites, setFavorites] = useState<FavoriteItem[]>([]);
    const [history, setHistory] = useState<HistoryItem[]>([]);
    const [pcsx2Installations, setPcsx2Installations] = useState<Pcsx2Installation[]>([]);
    const [pcsx2Profiles, setPcsx2Profiles] = useState<Pcsx2Profile[]>([]);

    const [selectedGame, setSelectedGame] = useState<{
        id: string;
        title: string;
        url: string;
        cover_url: string;
        size: number;
        region: string;
        part: string;
        format?: string;
        isInstalled?: boolean;
        installedPath?: string;
        installedId?: string;
        serial?: string | null;
    } | null>(null);
    const [detailSubTab, setDetailSubTab] = useState<"info" | "compatibility">("info");
    const [gameMetadata, setGameMetadata] = useState<GameMetadata | null>(null);
    const [isLoadingMetadata, setIsLoadingMetadata] = useState(false);
    const [integrityResult, setIntegrityResult] = useState<IntegrityResult | null>(null);
    const [isVerifyingIntegrity, setIsVerifyingIntegrity] = useState(false);

    const [loadErrors, setLoadErrors] = useState<Record<string, string>>({});
    const reportLoad = useCallback((key: string, error?: unknown) => {
        setLoadErrors(prev => {
            const next = { ...prev };
            if (error) next[key] = error instanceof Error ? error.message : "Erro de comunicação";
            else delete next[key];
            return next;
        });
    }, []);
    const catalogAbort = useRef<AbortController | null>(null);
    const searchAbort = useRef<AbortController | null>(null);
    const downloadsBusy = useRef(false);
    const completedIds = useRef(new Set<string>());
    const [isStartingDownload, setIsStartingDownload] = useState(false);
    const startingDownload = useRef(false);

    const fetchInstalledGames = useCallback(async () => {
        try {
            const data = await apiJson<InstalledGame[]>("/api/games");
            setInstalledGames(data);
            reportLoad("Biblioteca");
        } catch (error) {
            reportLoad("Biblioteca", error);
        }
    }, [reportLoad]);

    const scanGames = async () => {
        setIsScanning(true);
        try {
            const res = await apiFetch("/api/games/scan", { method: "POST" });
            const data = await res.json();
            const count = Array.isArray(data) ? data.length : (data.found || 0);
            showToast(`${count} jogos encontrados e vinculados com capas!`, "success");
            setLibrarySearch("");
            setLibraryFilterRegion("all");
            setLibraryFilterFormat("all");
            fetchInstalledGames();
        } catch {
            showToast("Falha ao escanear diretório de jogos do PCSX2.", "error");
        } finally {
            setIsScanning(false);
        }
    };

    const fetchCatalog = useCallback(async (p: number, tab: string, letFilter: string, q: string, pp: number, srt: string, reg: string) => {
        catalogAbort.current?.abort();
        const controller = new AbortController();
        catalogAbort.current = controller;
        setIsCatalogLoading(true);
        try {
            const params = new URLSearchParams({
                page: p.toString(),
                per_page: pp.toString(),
                search: q,
                sort: tab === "popular" ? "popular" : srt,
                letter: letFilter
            });
            if (reg && reg !== "all") {
                params.set("region", reg);
            }
            const data = await apiJson<CatalogResponse>(`/api/catalog?${params.toString()}`, { signal: controller.signal });
            if (!controller.signal.aborted) { setCatalog(data); reportLoad("Catálogo"); }
        } catch (error) {
            if (!controller.signal.aborted) reportLoad("Catálogo", error);
        } finally {
            if (!controller.signal.aborted) setIsCatalogLoading(false);
        }
    }, [reportLoad]);

    const executeSearch = useCallback(async (
        query: string,
        genre: string,
        lang: string,
        source: string,
        sort: string,
        pp: number,
        fmt: string
    ) => {
        searchAbort.current?.abort();
        const controller = new AbortController();
        searchAbort.current = controller;
        if (!query.trim() && !genre.trim() && lang === "all" && fmt === "all") {
            setSearchResults([]);
            setIsSearching(false);
            reportLoad("Pesquisa");
            return;
        }
        setIsSearching(true);
        try {
            if (source === "cdromance") {
                const params = new URLSearchParams({
                    q: [query.trim(), genre.trim()].filter(Boolean).join(" ") || "portuguese"
                });
                const data = await apiJson<RomCatalogItem[]>(`/api/roms/translations?${params.toString()}`, { signal: controller.signal });
                if (!controller.signal.aborted) setSearchResults(data);
            } else if (source === "all") {
                const catalogParams = new URLSearchParams({
                    q: query.trim(),
                    region: lang === "all" ? "" : lang,
                    category: genre.trim(),
                    sort: sort,
                    page: "1",
                    per_page: pp.toString()
                });
                if (fmt !== "all") catalogParams.set("format", fmt);
                const catalogPromise = apiJson<CatalogResponse>(`/api/catalog?${catalogParams.toString()}`, { signal: controller.signal });
                const translationsPromise = query.trim()
                    ? apiJson<RomCatalogItem[]>(`/api/roms/translations?${new URLSearchParams({ q: query.trim() }).toString()}`, { signal: controller.signal }).catch(() => [])
                    : Promise.resolve([]);
                const [catalogRes, transRes] = await Promise.all([catalogPromise, translationsPromise]);
                if (!controller.signal.aborted) {
                    const combined = [...(catalogRes?.items || []), ...(transRes || [])];
                    const seen = new Set<string>();
                    const deduplicated = combined.filter(item => {
                        const key = item.url || item.id;
                        if (seen.has(key)) return false;
                        seen.add(key);
                        return true;
                    });
                    setSearchResults(deduplicated);
                }
            } else {
                const params = new URLSearchParams({
                    q: query.trim(),
                    region: lang === "all" ? "" : lang,
                    category: genre.trim(),
                    sort: sort,
                    page: "1",
                    per_page: pp.toString()
                });
                if (fmt !== "all") params.set("format", fmt);
                const data = await apiJson<CatalogResponse>(`/api/catalog?${params.toString()}`, { signal: controller.signal });
                if (!controller.signal.aborted) setSearchResults(data.items);
            }
            reportLoad("Pesquisa");
        } catch (error) {
            if (!controller.signal.aborted) reportLoad("Pesquisa", error);
        } finally {
            if (!controller.signal.aborted) setIsSearching(false);
        }
    }, [reportLoad]);

    const fetchDownloads = useCallback(async () => {
        if (downloadsBusy.current) return;
        downloadsBusy.current = true;
        try {
            const [data, queueInfo] = await Promise.all([
                apiJson<DownloadItem[]>("/api/downloads"),
                apiJson<{ paused: boolean }>("/api/downloads/queue").catch(() => ({ paused: false }))
            ]);
            const completed = data.filter(d => d.status === "Completed");
            if (completed.some(d => !completedIds.current.has(d.id))) void fetchInstalledGames();
            completedIds.current = new Set(completed.map(d => d.id));
            setDownloads(data);
            setQueuePaused(Boolean(queueInfo.paused));
            reportLoad("Downloads");
        } catch (error) {
            reportLoad("Downloads", error);
        } finally { downloadsBusy.current = false; }
    }, [fetchInstalledGames, reportLoad]);

    const fetchConfigs = useCallback(async () => {
        try {
            const data = await apiJson<Record<string, string>>("/api/config");
            reportLoad("Configurações");
            setConfigs(prev => ({
                ...prev,
                ...data,
                roms_directory: data.roms_directory || data.roms_dir || prev.roms_directory,
                pcsx2_executable_path: data.pcsx2_executable_path || data.pcsx2_path || prev.pcsx2_executable_path,
                covers_directory: data.covers_directory || data.covers_dir || prev.covers_directory
            }));
        } catch (error) { reportLoad("Configurações", error); }
    }, [reportLoad]);

    const fetchLogs = useCallback(async () => {
        try {
            const data = await apiJson<string[]>("/api/logs");
            setLogs(data);
            reportLoad("Logs");
        } catch (error) {
            reportLoad("Logs", error);
        }
    }, [reportLoad]);

    const fetchFavorites = useCallback(async () => {
        try {
            const data = await apiJson<FavoriteItem[]>("/api/favorites");
            setFavorites(data);
            reportLoad("Favoritos");
        } catch (error) {
            reportLoad("Favoritos", error);
        }
    }, [reportLoad]);

    const fetchHistory = useCallback(async () => {
        try {
            const data = await apiJson<HistoryItem[]>("/api/history");
            setHistory(data);
            reportLoad("Histórico");
        } catch (error) {
            reportLoad("Histórico", error);
        }
    }, [reportLoad]);

    const fetchJobs = useCallback(async () => {
        try {
            const data = await apiJson<JobItem[]>("/api/jobs");
            setJobs(data);
            reportLoad("Atividade");
        } catch (error) {
            if (error instanceof Error && (error.message.includes("404") || error.message.includes("status 404"))) {
                setJobs([]);
                return;
            }
            reportLoad("Atividade", error);
        }
    }, [reportLoad]);

    const fetchPcsx2Data = useCallback(async () => {
        try {
            const [insts, profs] = await Promise.all([
                apiJson<Pcsx2Installation[]>("/api/pcsx2/detect").catch(() => []),
                apiJson<Pcsx2Profile[]>("/api/pcsx2/profiles").catch(() => []),
            ]);
            setPcsx2Installations(insts);
            setPcsx2Profiles(profs);
        } catch {
        }
    }, []);

    const handleSwitchPcsx2Version = async (path: string) => {
        try {
            const matchingInst = pcsx2Installations.find(i => i.path.toLowerCase() === path.toLowerCase());
            const res = await apiFetch("/api/pcsx2/version/select", {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({ executable_path: path, data_dir: matchingInst?.data_dir })
            });
            if (res.ok) {
                showToast("Versão do PCSX2 alterada!", "success");
                void fetchPcsx2Data();
                void fetchConfigs();
            } else {
                showToast("Falha ao selecionar versão do PCSX2.", "error");
            }
        } catch {
            showToast("Erro ao trocar versão do PCSX2.", "error");
        }
    };

    const handleSwitchPcsx2Profile = async (profileId: string) => {
        try {
            const res = await apiFetch(`/api/pcsx2/profiles/${profileId}/activate`, { method: "POST" });
            if (res.ok) {
                showToast("Perfil do PCSX2 ativado!", "success");
                void fetchPcsx2Data();
                void fetchConfigs();
            } else {
                showToast("Falha ao ativar perfil.", "error");
            }
        } catch {
            showToast("Erro ao trocar perfil do PCSX2.", "error");
        }
    };

    useEffect(() => {
        fetchInstalledGames();
        fetchDownloads();
        fetchConfigs();
        fetchLogs();
        fetchFavorites();
        fetchHistory();
        fetchJobs();
        fetchPcsx2Data();

        const timer = setInterval(() => {
            fetchDownloads();
            fetchJobs();
        }, 3000);
        return () => clearInterval(timer);
    }, [fetchInstalledGames, fetchDownloads, fetchConfigs, fetchLogs, fetchFavorites, fetchHistory, fetchJobs, fetchPcsx2Data]);

    useEffect(() => {
        const controller = new AbortController();
        setRomsValidation(null); setPcsx2Validation(null); setCoversValidation(null);
        const timer = setTimeout(() => {
            const paths = [
                ["roms-dir", configs.roms_directory, setRomsValidation],
                ["pcsx2-exe", configs.pcsx2_executable_path, setPcsx2Validation],
                ["covers-dir", configs.covers_directory, setCoversValidation],
            ] as const;
            for (const [kind, path, setter] of paths) {
                if (path) void apiJson<ValidationResult>(`/api/config/validate-${kind}?path=${encodeURIComponent(path)}`, { signal: controller.signal })
                    .then(result => { if (!controller.signal.aborted) setter(result); })
                    .catch(() => { if (!controller.signal.aborted) setter({ valid: false, message: "Não foi possível validar este caminho." }); });
            }
        }, 400);
        return () => { clearTimeout(timer); controller.abort(); };
    }, [configs.roms_directory, configs.pcsx2_executable_path, configs.covers_directory]);

    useEffect(() => {
        if (activeTab === "library" || activeTab === "emulator") {
            void fetchInstalledGames();
            void fetchPcsx2Data();
        }
        if (activeTab === "settings") void fetchLogs();
    }, [activeTab, fetchInstalledGames, fetchPcsx2Data, fetchLogs]);

    useEffect(() => () => { catalogAbort.current?.abort(); searchAbort.current?.abort(); }, []);

    useEffect(() => {
        const timer = setTimeout(() => void fetchCatalog(catalogPage, catalogTab, catalogLetter, catalogSearch, perPage, catalogSort, catalogRegion), 200);
        return () => clearTimeout(timer);
    }, [catalogPage, catalogTab, catalogLetter, catalogSearch, perPage, catalogSort, catalogRegion, fetchCatalog]);

    useEffect(() => {
        if (activeTab !== "search") return;
        const timer = setTimeout(() => {
            executeSearch(searchQuery, searchGenre, searchLang, searchSource, searchSort, searchPerPage, searchFormat);
        }, 200);
        return () => clearTimeout(timer);
    }, [activeTab, searchQuery, searchGenre, searchLang, searchSource, searchSort, searchPerPage, searchFormat, executeSearch]);

    useEffect(() => {
        setGameMetadata(null);
        setIsLoadingMetadata(false);
        if (!selectedGame) return;
        const controller = new AbortController();
        setIsLoadingMetadata(true);
        const params = new URLSearchParams({ title: selectedGame.title });
        if (selectedGame.serial) params.set("serial", selectedGame.serial);
        apiJson<GameMetadata>(`/api/game/metadata?${params}`, { signal: controller.signal })
            .then(data => { if (!controller.signal.aborted) setGameMetadata(data); })
            .catch(() => { if (!controller.signal.aborted) setGameMetadata(null); })
            .finally(() => { if (!controller.signal.aborted) setIsLoadingMetadata(false); });
        return () => controller.abort();
    }, [selectedGame]);

    const isActiveDownload = (status?: string) =>
        ["queued", "pending", "downloading", "extracting", "copying"].includes((status || "").toLowerCase());
    const isRunningDownload = (status?: string) =>
        ["downloading", "extracting", "copying"].includes((status || "").toLowerCase());
    const isQueuedDownload = (status?: string) =>
        ["queued", "pending"].includes((status || "").toLowerCase());
    const isHistoryDownload = (status?: string) =>
        ["completed", "cancelled", "error"].includes((status || "").toLowerCase());

    const activeDownloadsCount = downloads.filter(d => isActiveDownload(d.status)).length;
    const activeJobsCount = jobs.filter(j => j.status === "running" || j.status === "cancel_requested").length;

    const isItemInstalled = useCallback((item: RomCatalogItem) => {
        return installedGames.some(
            game =>
                (item.filename && game.filename.toLowerCase() === item.filename.toLowerCase()) ||
                game.title.toLowerCase() === item.title.toLowerCase()
        );
    }, [installedGames]);

    const isItemDownloadingOrQueued = useCallback((item: RomCatalogItem) => {
        return downloads.some(
            d => d.url === item.url && isActiveDownload(d.status)
        );
    }, [downloads]);

    const toggleBulkSelect = (item: RomCatalogItem) => {
        if (isItemInstalled(item) || isItemDownloadingOrQueued(item)) return;
        setSelectedBulk(prev => {
            const next = new Map(prev);
            if (next.has(item.id)) next.delete(item.id);
            else next.set(item.id, item);
            return next;
        });
    };

    const selectAllCurrentList = (list: RomCatalogItem[]) => {
        let skippedCount = 0;
        let addedCount = 0;
        setSelectedBulk(prev => {
            const next = new Map(prev);
            for (const item of list) {
                if (isItemInstalled(item) || isItemDownloadingOrQueued(item)) {
                    skippedCount++;
                    continue;
                }
                if (!next.has(item.id)) {
                    next.set(item.id, item);
                    addedCount++;
                }
            }
            return next;
        });
        if (skippedCount > 0 && addedCount === 0) {
            showToast("Todos os jogos desta lista já estão instalados ou na fila.", "info");
        }
    };

    const clearBulkSelection = () => setSelectedBulk(new Map());

    const startBulkDownloads = async () => {
        if (startingDownload.current || selectedBulk.size === 0) return;
        startingDownload.current = true;
        setIsStartingDownload(true);
        try {
            const allSelected = Array.from(selectedBulk.values());
            const eligible = allSelected.filter(item => !isItemInstalled(item) && !isItemDownloadingOrQueued(item));
            const preFilteredSkipped = allSelected.length - eligible.length;
            if (eligible.length === 0) {
                showToast("Os jogos selecionados já estão instalados ou na fila.", "info");
                setSelectedBulk(new Map());
                setIsBulkSelectActive(false);
                return;
            }
            const res = await apiFetch("/api/downloads/bulk", {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({
                    items: eligible.map(item => ({
                        title: item.title,
                        url: item.url,
                        cover_url: item.cover_url,
                        size: item.size
                    }))
                })
            });
            if (!res.ok) {
                const data = await res.json().catch(() => ({}));
                showToast(data.error || "Erro ao iniciar downloads em lote.", "error");
                return;
            }
            const result = await res.json().catch(() => ({ added: eligible.length, skipped: 0 }));
            const totalAdded = Number(result.added ?? eligible.length);
            const totalSkipped = Number(result.skipped ?? 0) + preFilteredSkipped;
            const msg = totalSkipped > 0
                ? `${totalAdded} ${totalAdded === 1 ? "jogo adicionado" : "jogos adicionados"} à fila. ${totalSkipped} ${totalSkipped === 1 ? "ignorado pois já estava instalado" : "ignorados pois já estavam instalados"}.`
                : `${totalAdded} ${totalAdded === 1 ? "jogo adicionado" : "jogos adicionados"} à fila de download.`;
            showToast(msg, "success");
            setSelectedBulk(new Map());
            setIsBulkSelectActive(false);
            void fetchDownloads();
            setActiveTab("downloads");
        } catch (error) {
            showToast(error instanceof Error ? error.message : "Erro ao enfileirar downloads.", "error");
        } finally {
            startingDownload.current = false;
            setIsStartingDownload(false);
        }
    };

    const pauseQueue = async () => {
        try {
            await apiFetch("/api/downloads/queue/pause", { method: "POST" });
            setQueuePaused(true);
            showToast("Fila de downloads pausada.", "info");
        } catch {
            showToast("Não foi possível pausar a fila.", "error");
        }
    };

    const resumeQueue = async () => {
        try {
            await apiFetch("/api/downloads/queue/resume", { method: "POST" });
            setQueuePaused(false);
            void fetchDownloads();
            showToast("Fila de downloads retomada.", "success");
        } catch {
            showToast("Não foi possível retomar a fila.", "error");
        }
    };

    const cancelAllQueued = async () => {
        try {
            const res = await apiJson<{ cancelled: number }>("/api/downloads/queue/cancel-all", { method: "POST" });
            void fetchDownloads();
            showToast(`${res.cancelled} ${res.cancelled === 1 ? "item pendente cancelado" : "itens pendentes cancelados"}.`, "info");
        } catch {
            showToast("Erro ao cancelar itens da fila.", "error");
        }
    };

    useEffect(() => {
        if (activeTab !== "downloads" && activeDownloadsCount === 0) return;
        const timer = setInterval(() => {
            fetchDownloads();
        }, 500);
        return () => clearInterval(timer);
    }, [activeTab, activeDownloadsCount, fetchDownloads]);

    const startDownload = async (title: string, url: string, coverUrl?: string, size?: number) => {
        if (startingDownload.current) return;
        startingDownload.current = true;
        setIsStartingDownload(true);
        try {
            const res = await apiFetch("/api/downloads", {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({
                    title,
                    url,
                    cover_url: coverUrl,
                    size
                })
            });
            if (!res.ok) {
                const data = await res.json().catch(() => ({}));
                showToast(data.error || "Erro ao iniciar download.", "error");
                return;
            }
            showToast(`Download de "${title}" iniciado!`, "success");
            fetchDownloads();
            setActiveTab("downloads");
            setSelectedGame(null);
        } catch (error) {
            showToast(error instanceof Error ? error.message : "Erro ao iniciar download.", "error");
        } finally { startingDownload.current = false; setIsStartingDownload(false); }
    };

    const cancelDownload = async (id: string) => {
        try {
            await apiFetch(`/api/downloads/${id}/cancel`, { method: "POST" });
            showToast("Download cancelado.", "info");
            fetchDownloads();
        } catch (error) {
            showToast(error instanceof Error ? error.message : "Erro ao cancelar download.", "error");
        }
    };

    const removeDownload = async (id: string) => {
        try {
            await apiFetch(`/api/downloads/${id}`, { method: "DELETE" });
            fetchDownloads();
        } catch (error) {
            showToast(error instanceof Error ? error.message : "Erro ao remover item.", "error");
        }
    };

    const retryDownload = async (id: string) => {
        try {
            await apiFetch(`/api/downloads/${id}/retry`, { method: "POST" });
            showToast("Recomeçando download...", "info");
            fetchDownloads();
        } catch (error) {
            showToast(error instanceof Error ? error.message : "Erro ao reiniciar download.", "error");
        }
    };

    const clearDownloadHistory = async () => {
        const historyDownloads = downloads.filter(d => ["Completed", "Cancelled", "Error"].includes(d.status));
        const results = await Promise.allSettled(historyDownloads.map(d => apiFetch(`/api/downloads/${d.id}`, { method: "DELETE" })));
        void fetchDownloads();
        const failed = results.filter(r => r.status === "rejected").length;
        showToast(failed ? `${failed} itens não puderam ser removidos. Tente novamente.` : "Histórico de downloads limpo.", failed ? "error" : "success");
    };

    const executeDeleteGame = async () => {
        if (!gameToDelete) return;
        setIsDeletingGame(true);
        try {
            const res = await apiFetch(`/api/games/${encodeURIComponent(gameToDelete.id)}`, {
                method: "DELETE"
            });
            if (!res.ok) {
                throw new Error("Falha ao deletar jogo");
            }
            showToast(`"${gameToDelete.title}" foi excluído do disco. As capas foram preservadas.`, "success");
            setGameToDelete(null);

            if (selectedGame && (selectedGame.installedId || selectedGame.id) === gameToDelete.id) {
                setSelectedGame(null);
                setActiveTab("library");
            }
            fetchInstalledGames();
        } catch {
            showToast("Erro ao deletar jogo e seus arquivos.", "error");
        } finally {
            setIsDeletingGame(false);
        }
    };

    const handleSaveRomsDir = async () => {
        setIsSavingConfig(true);
        const val = configs.roms_directory || "";
        try {
            const vRes = await apiJson<ValidationResult>(`/api/config/validate-roms-dir?path=${encodeURIComponent(val)}`);
            setRomsValidation(vRes);
            if (!vRes.valid) { showToast(vRes.message, "error"); return; }
            await apiFetch("/api/config", {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({ key: "roms_directory", value: val })
            });
            if (vRes.valid) {
                showToast("Pasta de jogos validada e salva com sucesso!", "success");
                scanGames();
            } else {
                showToast("Aviso: diretório de jogos não localizado.", "error");
            }
        } catch {
            setRomsValidation({ valid: false, message: "Erro de comunicação ao validar pasta de jogos." });
        } finally {
            setIsSavingConfig(false);
        }
    };

    const handleSavePcsx2Exe = async () => {
        setIsSavingConfig(true);
        const val = configs.pcsx2_executable_path || "";
        try {
            const vRes = await apiJson<ValidationResult>(`/api/config/validate-pcsx2-exe?path=${encodeURIComponent(val)}`);
            setPcsx2Validation(vRes);
            if (!vRes.valid) { showToast(vRes.message, "error"); return; }
            await apiFetch("/api/config", {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({ key: "pcsx2_executable_path", value: val })
            });
            if (vRes.valid) {
                showToast("Executável do PCSX2 validado e salvo com sucesso!", "success");
            } else {
                showToast("Executável não localizado no caminho informado.", "error");
            }
        } catch {
            setPcsx2Validation({ valid: false, message: "Erro ao validar executável." });
        } finally {
            setIsSavingConfig(false);
        }
    };

    const handleSaveCoversDir = async () => {
        setIsSavingConfig(true);
        const val = configs.covers_directory || "";
        try {
            const vRes = await apiJson<ValidationResult>(`/api/config/validate-covers-dir?path=${encodeURIComponent(val)}`);
            setCoversValidation(vRes);
            if (!vRes.valid) { showToast(vRes.message, "error"); return; }
            await apiFetch("/api/config", {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({ key: "covers_directory", value: val })
            });
            if (vRes.valid) {
                showToast("Pasta de capas validada e salva com sucesso!", "success");
            } else {
                showToast("Pasta de capas não localizada.", "error");
            }
        } catch {
            setCoversValidation({ valid: false, message: "Erro ao validar pasta de capas." });
        } finally {
            setIsSavingConfig(false);
        }
    };

    const openFolder = async (path: string) => {
        if (!path) return;
        try {
            await apiFetch(`/api/open-folder?path=${encodeURIComponent(path)}`);
        } catch {
            showToast("Não foi possível abrir a pasta.", "error");
        }
    };

    const launchGame = async (gamePath: string) => {
        try {
            const res = await apiFetch("/api/games/run", {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({ game_path: gamePath })
            });
            if (res.ok) {
                showToast("Jogo iniciado no PCSX2 com sucesso!", "success");
                void fetchHistory();
            } else {
                showToast("Erro ao iniciar PCSX2. Verifique o executável nas Configurações.", "error");
            }
        } catch {
            showToast("Erro de comunicação ao iniciar emulador.", "error");
        }
    };

    const toggleFavorite = async (game: { id: string; title: string; cover_url?: string | null; url: string; size?: number | null; region?: string | null }) => {
        const exists = favorites.some(f => f.id === game.id);
        try {
            if (exists) {
                await apiFetch(`/api/favorites/${encodeURIComponent(game.id)}`, { method: "DELETE" });
                showToast("Removido dos favoritos.", "info");
            } else {
                await apiFetch("/api/favorites", {
                    method: "POST",
                    headers: { "Content-Type": "application/json" },
                    body: JSON.stringify({
                        id: game.id,
                        title: game.title,
                        cover_url: game.cover_url || null,
                        url: game.url,
                        size: game.size || null,
                        region: game.region || null
                    })
                });
                showToast("Adicionado aos favoritos.", "success");
            }
            fetchFavorites();
        } catch (error) {
            showToast(error instanceof Error ? error.message : "Erro ao atualizar favoritos.", "error");
        }
    };

    const openInstalledGame = (game: InstalledGame) => {
        setIntegrityResult(null);
        setSelectedGame({ id:game.id,title:game.title,url:"",cover_url:game.cover_url || "",size:game.file_size,region:game.region || "Desconhecido",part:game.serial || "",serial:game.serial,format:game.format,isInstalled:true,installedPath:game.file_path,installedId:game.id });
        setPreviousTab(activeTab);setActiveTab("game-page");
    };

    const regenerateCovers = async () => {
        try {
            const job=await apiJson<CoverJob>("/api/games/covers/regenerate", {method:"POST"});
            setCoverJob(job);showToast("Regeneração iniciada. As imagens pessoais serão preservadas.","info");
        } catch(error) {showToast(error instanceof Error?error.message:"Não foi possível regenerar capas.","error");}
    };
    const saveAutoCovers = async (enabled:boolean) => {
        try {await apiFetch("/api/config",{method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify({key:"auto_download_covers",value:String(enabled)})});setConfigs(current=>({...current,auto_download_covers:String(enabled)}));showToast("Preferência salva.","success");}
        catch(error){showToast(error instanceof Error?error.message:"Não foi possível salvar a preferência.","error");}
    };
    const saveMaxConcurrent = async (value:string) => {
        try {await apiFetch("/api/config",{method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify({key:"max_concurrent_downloads",value})});setConfigs(current=>({...current,max_concurrent_downloads:value}));void fetchDownloads();showToast("Limite de downloads simultâneos salvo.","success");}
        catch(error){showToast(error instanceof Error?error.message:"Não foi possível salvar a preferência.","error");}
    };
    const clearLogs = async () => {
        try {await apiFetch("/api/logs",{method:"DELETE"});await fetchLogs();showToast("Logs limpos.","success");}
        catch(error){showToast(error instanceof Error?error.message:"Não foi possível limpar os logs.","error");}
    };

    const handleWizardSkipAll = useCallback(() => {
        try { localStorage.setItem("setup_done", "1"); } catch {}
        setShowSetupWizard(false);
    }, []);

    useEffect(() => {
        if (!showSetupWizard) return;
        const onKeyDown = (event: KeyboardEvent) => {
            if (event.key === "Escape" && !wizardIsFinishing) {
                handleWizardSkipAll();
            }
        };
        window.addEventListener("keydown", onKeyDown);
        return () => window.removeEventListener("keydown", onKeyDown);
    }, [showSetupWizard, wizardIsFinishing, handleWizardSkipAll]);

    const checkWizardPath = async (kind: "roms-dir" | "pcsx2-exe" | "covers-dir", rawPath: string) => {
        const trimmed = rawPath.trim();
        const setter = kind === "roms-dir" ? setWizardRomsCheck : kind === "pcsx2-exe" ? setWizardPcsx2Check : setWizardCoversCheck;
        if (!trimmed) {
            setter(null);
            return;
        }
        try {
            const result = await apiJson<ValidationResult>(`/api/config/validate-${kind}?path=${encodeURIComponent(trimmed)}`);
            setter(result);
        } catch {
            setter({ valid: false, message: "Não foi possível validar este caminho." });
        }
    };

    const handleOpenWizard = () => {
        setWizardRomsDir(configs.roms_directory || "");
        setWizardPcsx2Path(pcsx2Validation?.valid ? (configs.pcsx2_executable_path || "") : "");
        setWizardCoversDir(configs.covers_directory || "");
        setWizardAutoCovers(configs.auto_download_covers !== "false");
        setWizardMaxConcurrent(configs.max_concurrent_downloads || "2");
        setWizardRomsCheck(configs.roms_directory ? romsValidation : null);
        setWizardPcsx2Check(pcsx2Validation?.valid ? pcsx2Validation : null);
        setWizardCoversCheck(configs.covers_directory ? coversValidation : null);
        setWizardStep(0);
        setShowSetupWizard(true);
    };

    const handleWizardNext = async () => {
        if (wizardStep < 3) {
            setWizardStep(prev => prev + 1);
            return;
        }
        setWizardIsFinishing(true);
        const saveConfigKey = (key: string, value: string) =>
            apiFetch("/api/config", {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({ key, value })
            }).catch(() => {});

        await saveConfigKey("auto_download_covers", String(wizardAutoCovers));
        await saveConfigKey("max_concurrent_downloads", wizardMaxConcurrent);
        setConfigs(current => ({
            ...current,
            auto_download_covers: String(wizardAutoCovers),
            max_concurrent_downloads: wizardMaxConcurrent
        }));

        if (wizardRomsDir.trim()) {
            const romsPath = wizardRomsDir.trim();
            try {
                const vRes = await apiJson<ValidationResult>(`/api/config/validate-roms-dir?path=${encodeURIComponent(romsPath)}`);
                setRomsValidation(vRes);
                if (vRes.valid) {
                    await saveConfigKey("roms_directory", romsPath);
                    setConfigs(current => ({ ...current, roms_directory: romsPath }));
                    void scanGames();
                } else {
                    showToast(`Pasta de ROMs inválida: ${vRes.message}. Configure depois em Configurações.`, "warning");
                }
            } catch {
                showToast("Não foi possível validar a pasta de ROMs. Configure depois em Configurações.", "warning");
            }
        }

        if (wizardPcsx2Path.trim()) {
            const pcsx2Path = wizardPcsx2Path.trim();
            try {
                const vRes = await apiJson<ValidationResult>(`/api/config/validate-pcsx2-exe?path=${encodeURIComponent(pcsx2Path)}`);
                setPcsx2Validation(vRes);
                if (vRes.valid) {
                    await saveConfigKey("pcsx2_executable_path", pcsx2Path);
                    setConfigs(current => ({ ...current, pcsx2_executable_path: pcsx2Path }));
                } else {
                    showToast(`Executável do PCSX2 inválido: ${vRes.message}. Configure depois em Configurações.`, "warning");
                }
            } catch {
                showToast("Não foi possível validar o executável do PCSX2. Configure depois em Configurações.", "warning");
            }
        }

        if (wizardCoversDir.trim()) {
            const coversPath = wizardCoversDir.trim();
            try {
                const vRes = await apiJson<ValidationResult>(`/api/config/validate-covers-dir?path=${encodeURIComponent(coversPath)}`);
                setCoversValidation(vRes);
                if (vRes.valid) {
                    await saveConfigKey("covers_directory", coversPath);
                    setConfigs(current => ({ ...current, covers_directory: coversPath }));
                } else {
                    showToast(`Pasta de capas inválida: ${vRes.message}. Configure depois em Configurações.`, "warning");
                }
            } catch {
                showToast("Não foi possível validar a pasta de capas. Configure depois em Configurações.", "warning");
            }
        }

        try { localStorage.setItem("setup_done", "1"); } catch {}
        setShowSetupWizard(false);
        setWizardIsFinishing(false);
    };
    useEffect(()=>{const controller=new AbortController();void apiJson<CoverJob>("/api/games/covers/regenerate",{signal:controller.signal}).then(setCoverJob).catch(()=>undefined);return()=>controller.abort();},[]);
    useEffect(()=>{
        if(!coverJob?.running)return;
        const controller=new AbortController();let timer:ReturnType<typeof setTimeout>;
        const poll=async()=>{
            try {
                const job=await apiJson<CoverJob>("/api/games/covers/regenerate",{signal:controller.signal});
                if(controller.signal.aborted)return;reportLoad("Capas");
                if(job.running){setCoverJob(job);timer=setTimeout(()=>void poll(),1000);}
                else {
                    await fetchInstalledGames();
                    const config=await apiJson<Record<string,string>>("/api/config",{signal:controller.signal});
                    setConfigs(current=>({...current,covers_directory:current.covers_directory || config.covers_directory || config.covers_dir || ""}));
                    showToast(job.error || `Capas: ${job.updated} atualizadas, ${job.preserved} pessoais preservadas, ${job.failed} sem atualização.`,job.error?"error":job.failed?"warning":"success");
                    setCoverJob(job);
                }
            }catch(error){if(!controller.signal.aborted){reportLoad("Capas",error);timer=setTimeout(()=>void poll(),2000);}}
        };
        timer=setTimeout(()=>void poll(),600);
        return()=>{controller.abort();clearTimeout(timer);};
    },[coverJob?.running,fetchInstalledGames,reportLoad]);

    const openGameDetail = (game: RomCatalogItem) => {
        setIntegrityResult(null);
        const installed = installedGames.find(g => g.filename.toLowerCase() === game.filename.toLowerCase() || g.title.toLowerCase() === game.title.toLowerCase());
        setSelectedGame({
            id: game.id,
            title: game.title,
            url: game.url,
            cover_url: game.cover_url,
            size: game.size,
            region: game.region,
            part: game.part,
            isInstalled: !!installed,
            installedPath: installed?.file_path,
            installedId: installed?.id,
            serial: installed?.serial
        });
        setPreviousTab(activeTab);
        setActiveTab("game-page");
    };

    const checkIntegrity = async (gameId: string) => {
        setIsVerifyingIntegrity(true);
        try {
            const res = await apiJson<IntegrityResult>(`/api/games/${encodeURIComponent(gameId)}/verify`);
            setIntegrityResult(res);
            if (res.healthy) {
                showToast("Arquivo de ROM íntegro e verificado no disco!", "success");
            } else {
                showToast(res.message || "Problema de integridade detectado.", "warning");
            }
        } catch (error) {
            showToast(error instanceof Error ? error.message : "Não foi possível verificar a integridade.", "error");
        } finally {
            setIsVerifyingIntegrity(false);
        }
    };

    const filteredInstalledGames = installedGames
        .filter(g => {
            if (librarySearch && !g.title.toLowerCase().includes(librarySearch.toLowerCase())) return false;
            if (libraryFilterRegion !== "all" && g.region !== libraryFilterRegion) return false;
            if (libraryFilterFormat !== "all" && g.format.toLowerCase() !== libraryFilterFormat.toLowerCase()) return false;
            if (libraryFilterStatus === "available" && g.is_missing) return false;
            if (libraryFilterStatus === "missing" && !g.is_missing) return false;
            return true;
        })
        .sort((a, b) => {
            if (librarySort === "name_asc") return a.title.localeCompare(b.title);
            if (librarySort === "name_desc") return b.title.localeCompare(a.title);
            if (librarySort === "size_desc") return b.file_size - a.file_size;
            if (librarySort === "size_asc") return a.file_size - b.file_size;
            return 0;
        });

    useEffect(() => {
        setSelectedGame(current => {
            if (!current) return current;
            const installed = installedGames.find(g => g.title.toLowerCase() === current.title.toLowerCase());
            if (!installed || current.installedPath === installed.file_path && current.cover_url === installed.cover_url) return current;
            return { ...current, isInstalled: true, installedPath: installed.file_path, installedId: installed.id, serial: installed.serial, cover_url:installed.cover_url || current.cover_url };
        });
    }, [installedGames]);

    const runningDownloadsList = downloads.filter(d => isRunningDownload(d.status));
    const queuedDownloadsList = downloads.filter(d => isQueuedDownload(d.status)).slice().reverse();
    const historyDownloadsList = downloads.filter(d => isHistoryDownload(d.status));
    const bulkSelectedTotalBytes = Array.from(selectedBulk.values()).reduce((sum, item) => sum + (item.size || 0), 0);

    return (
        <div className="app">
            <ToastContainer />
            <aside className="sidebar">
                <div className="brand">
                    <img src={logoUrl} alt="RomManager" className="brand-icon" />
                    <span>RomManager</span>
                </div>

                <div className="section-label">PLAYSTATION 2</div>
                <nav className="nav nav-group" aria-label="Navegação PlayStation 2">
                    {([
                        { id: "catalog" as Tab, label: "Catálogo PS2" },
                        { id: "search" as Tab, label: "Pesquisa" },
                        { id: "downloads" as Tab, label: "Downloads" }
                    ] as const).map(item => (
                        <button
                            key={item.id}
                            className={`nav-item${activeTab === item.id ? " active" : ""}`}
                            aria-current={activeTab === item.id ? "page" : undefined}
                            onClick={() => setActiveTab(item.id)}
                        >
                            <span className="nav-icon">{renderNavIcon(item.id)}</span>
                            <span>{item.label}</span>
                            {item.id === "downloads" && activeDownloadsCount > 0 && (
                                <span className="nav-badge">{activeDownloadsCount}</span>
                            )}
                        </button>
                    ))}
                </nav>

                <div className="section-label">EMULADOR · PCSX2</div>
                <nav className="nav nav-group" aria-label="Navegação Emulador PCSX2">
                    {([
                        { id: "library" as Tab, label: "Biblioteca" },
                        { id: "emulator" as Tab, label: "Instalação e Perfis" },
                        { id: "bios" as Tab, label: "BIOS" }
                    ] as const).map(item => (
                        <button
                            key={item.id}
                            className={`nav-item${activeTab === item.id ? " active" : ""}`}
                            aria-current={activeTab === item.id ? "page" : undefined}
                            onClick={() => setActiveTab(item.id)}
                        >
                            <span className="nav-icon">{renderNavIcon(item.id)}</span>
                            <span>{item.label}</span>
                        </button>
                    ))}
                </nav>

                <div className="section-label">GERAL</div>
                <nav className="nav nav-group" aria-label="Navegação Geral">
                    {([
                        { id: "activity" as Tab, label: "Atividade" },
                        { id: "profile" as Tab, label: "Perfil" },
                        { id: "settings" as Tab, label: "Configurações" }
                    ] as const).map(item => (
                        <button
                            key={item.id}
                            className={`nav-item${activeTab === item.id ? " active" : ""}`}
                            aria-current={activeTab === item.id ? "page" : undefined}
                            onClick={() => {
                                if (item.id === "profile") {
                                    fetchFavorites();
                                    fetchHistory();
                                }
                                if (item.id === "activity") {
                                    void fetchJobs();
                                }
                                setActiveTab(item.id);
                            }}
                        >
                            <span className="nav-icon">{renderNavIcon(item.id)}</span>
                            <span>{item.label}</span>
                            {item.id === "activity" && activeJobsCount > 0 && (
                                <span className="nav-badge">{activeJobsCount}</span>
                            )}
                        </button>
                    ))}
                </nav>
            </aside>

            <main className="main">
                <header className="topbar">
                    <div className="topbar-left">
                        {activeTab === "catalog" && (
                            <div className="result-count">
                                {isCatalogLoading ? "Carregando…" : `${catalog.total.toLocaleString("pt-BR")} jogos carregados · Página ${catalogPage}`}
                            </div>
                        )}
                        {activeTab === "search" && (
                            <div className="result-count">
                                {isSearching ? "Buscando…" : `${searchResults.length} jogos encontrados`}
                            </div>
                        )}
                        {activeTab === "library" && (
                            <div className="result-count">
                                {filteredInstalledGames.length} jogos instalados
                            </div>
                        )}
                        {activeTab === "downloads" && (
                            <div className="result-count">
                                {activeDownloadsCount} downloads ativos
                            </div>
                        )}
                        {activeTab === "emulator" && <div className="result-count">Gestão do Emulador</div>}
                        {activeTab === "bios" && <div className="result-count">Gestão de BIOS</div>}
                        {activeTab === "activity" && <div className="result-count">Central de Atividade</div>}
                        {activeTab === "settings" && <div className="result-count">Configurações</div>}
                        {activeTab === "profile" && <div className="result-count">Perfil</div>}
                    </div>
                </header>
                <div className="content">
                    {Object.keys(loadErrors).length>0 && <div className="load-errors" role="alert"><strong>Não foi possível atualizar alguns dados. Os últimos resultados foram mantidos.</strong>{Object.entries(loadErrors).map(([key,message])=><p key={key}>{key}: {message}</p>)}<button type="button" onClick={()=>{void fetchInstalledGames();void fetchDownloads();void fetchFavorites();void fetchHistory();void fetchLogs();if(loadErrors["Configurações"])void fetchConfigs();void fetchCatalog(catalogPage,catalogTab,catalogLetter,catalogSearch,perPage,catalogSort,catalogRegion);}}>Tentar novamente</button></div>}
                    {activeTab === "catalog" && <>
                        <section className="catalog-heading panel"><div><p className="field-hint">PLAYSTATION 2</p><h1>Catálogo PS2</h1></div><span className="field-hint">{catalog.total.toLocaleString("pt-BR")} resultados</span></section>
                        <div className="catalog-toolbar">
                            <div className="sort-tabs"><button type="button" className={`sort-tab-btn${catalogTab==="popular"?" active":""}`} onClick={()=>{setCatalogTab("popular");setCatalogLetter("");setCatalogPage(1);}}>Populares</button><button type="button" className={`sort-tab-btn${catalogTab==="az"?" active":""}`} onClick={()=>{setCatalogTab("az");setCatalogPage(1);}}>Todos (A–Z)</button></div>
                            <div className="catalog-toolbar-right">
                                <label className="inline-field">Região<select value={catalogRegion} onChange={event=>{setCatalogRegion(event.target.value);setCatalogPage(1);}}><option value="all">Todas as regiões</option><option value="USA">USA</option><option value="Europe">Europa</option><option value="Japan">Japão</option></select></label>
                                {catalogTab==="az" && <label className="inline-field">Ordem<select value={catalogSort} onChange={event=>{setCatalogSort(event.target.value as "title_asc"|"title_desc");setCatalogPage(1);}}><option value="title_asc">A–Z</option><option value="title_desc">Z–A</option></select></label>}
                                {!isBulkSelectActive ? (
                                    <button type="button" className="button-secondary" onClick={()=>setIsBulkSelectActive(true)}>Selecionar múltiplos</button>
                                ) : (
                                    <>
                                        <button type="button" className="button-secondary" disabled={catalog.items.length===0} onClick={()=>selectAllCurrentList(catalog.items)}>Selecionar página</button>
                                        {selectedBulk.size>0 && <button type="button" className="button-quiet" onClick={clearBulkSelection}>Limpar seleção ({selectedBulk.size})</button>}
                                        <button type="button" className="button-quiet" onClick={()=>{setIsBulkSelectActive(false);clearBulkSelection();}}>Cancelar seleção</button>
                                    </>
                                )}
                            </div>
                        </div>
                        <div className="alphabet-bar">
                            <nav className="alphabet-nav" aria-label="Índice alfabético">{["", "#", ..."ABCDEFGHIJKLMNOPQRSTUVWXYZ"].map(letter=><button type="button" key={letter || "all"} className={`alphabet-btn${catalogTab==="az" && catalogLetter===letter?" active":""}`} aria-pressed={catalogTab==="az" && catalogLetter===letter} onClick={()=>{setCatalogTab("az");setCatalogLetter(letter);setCatalogPage(1);}}>{letter || "Todos"}</button>)}</nav>
                            <div className="catalog-tools">
                                <label className="visually-hidden" htmlFor="catalog-search">Buscar no catálogo</label>
                                <input id="catalog-search" className="compact-input" type="search" value={catalogSearch} placeholder="Buscar no catálogo…" onChange={event=>{setCatalogSearch(event.target.value);setCatalogPage(1);}} />
                                <label className="catalog-page-size">Por página<select value={perPage} onChange={event=>{setPerPage(Number(event.target.value));setCatalogPage(1);}}>{PER_PAGE_OPTIONS.map(value=><option key={value}>{value}</option>)}</select></label>
                            </div>
                        </div>
                        {isCatalogLoading?<div className="loader-state" role="status"><div className="spinner"/><p>Carregando catálogo…</p></div>:catalog.items.length===0?<div className="empty-state"><h3>Nenhum título encontrado</h3><p>Nenhum jogo corresponde aos filtros ou pesquisa ativos.</p>{(catalogSearch || catalogRegion!=="all" || catalogLetter) && <button type="button" className="button-secondary" style={{marginTop:"0.75rem"}} onClick={()=>{setCatalogSearch("");setCatalogRegion("all");setCatalogLetter("");setCatalogTab("az");setCatalogPage(1);}}>Limpar filtros do catálogo</button>}</div>:<div className="mods-grid">{catalog.items.map(item=>{
                            const installed=isItemInstalled(item);
                            const inQueue=isItemDownloadingOrQueued(item);
                            const disabled=installed || inQueue;
                            const isSelected=selectedBulk.has(item.id);
                            return <article className={`mod-card${isSelected?" selected":""}`} key={item.id}>
                                <div className="mod-thumb-wrap">
                                    {isBulkSelectActive && (
                                        <button
                                            type="button"
                                            className={`card-select-btn${disabled?" disabled":""}${isSelected?" checked":""}`}
                                            disabled={disabled}
                                            onClick={(e)=>{e.stopPropagation();toggleBulkSelect(item);}}
                                            title={installed?"Já instalado na biblioteca":inQueue?"Já na fila ou em download":isSelected?"Desmarcar":"Selecionar para download"}
                                            aria-label={`Selecionar ${item.title}`}
                                            aria-pressed={isSelected}
                                        >
                                            {isSelected && (
                                                <svg viewBox="0 0 14 14" width="13" height="13" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" strokeLinejoin="round">
                                                    <polyline points="2.5 7.5 5.5 10.5 11.5 3.5" />
                                                </svg>
                                            )}
                                        </button>
                                    )}
                                    <button type="button" className="catalog-cover-button mod-thumb" aria-label={`Detalhes de ${item.title}`} onClick={()=>isBulkSelectActive ? toggleBulkSelect(item) : openGameDetail(item)}><CoverImage src={item.cover_url} title={item.title} region={item.region}/>{installed && <span className="mod-badge-installed">Instalado</span>}{!installed && inQueue && <span className="mod-badge-queued">Na fila</span>}</button>
                                </div>
                                <div className="mod-meta">
                                    <h3 className="mod-title"><button type="button" className="card-open" onClick={()=>openGameDetail(item)}>{item.title}</button></h3>
                                    <div className="card-badges-row">
                                        <span className="card-badge-chip card-badge-format">{item.format || (item.filename?.toLowerCase().endsWith(".7z") ? "7Z" : item.filename?.toLowerCase().endsWith(".iso") ? "ISO" : "CHD")}</span>
                                        <span className="card-badge-chip card-badge-region">{item.region || "Livre"}</span>
                                    </div>
                                    <p className="mod-author">{item.size>0 ? formatBytes(item.size) : "Tamanho sob consulta"} · Coleção: {item.part}</p>
                                    <div className="card-action-bar">
                                        {installed ? (
                                            <button type="button" className="button-secondary" onClick={()=>{setLibrarySearch(item.title);setActiveTab("library");}}>Ver na biblioteca</button>
                                        ) : inQueue ? (
                                            <button type="button" className="button-secondary" onClick={()=>setActiveTab("downloads")}>Ver na fila</button>
                                        ) : (
                                            <button type="button" className="button-secondary" disabled={isStartingDownload || disabled} onClick={()=>void startDownload(item.title,item.url,item.cover_url,item.size)}>Baixar</button>
                                        )}
                                    </div>
                                </div>
                            </article>;
                        })}</div>}
                        <nav className="pagination" aria-label="Paginação do catálogo">
                            <button type="button" className="page-btn" disabled={catalogPage<=1} onClick={()=>setCatalogPage(1)}>Primeira</button>
                            <button type="button" className="page-btn" disabled={catalogPage<=1} onClick={()=>setCatalogPage(current=>Math.max(1,current-1))}>Anterior</button>
                            <span className="pagination-current">{catalogPage} de {catalog.total_pages}</span>
                            <button type="button" className="page-btn" disabled={catalogPage>=catalog.total_pages} onClick={()=>setCatalogPage(current=>Math.min(catalog.total_pages,current+1))}>Próxima</button>
                            <button type="button" className="page-btn" disabled={catalogPage>=catalog.total_pages} onClick={()=>setCatalogPage(catalog.total_pages)}>Última</button>
                            <form className="page-jump" onSubmit={event=>{event.preventDefault();const target=Number(jumpPage);if(Number.isInteger(target)&&target>=1&&target<=catalog.total_pages){setCatalogPage(target);setJumpPage("");}}}>
                                <label htmlFor="jump-page">Ir para</label>
                                <input id="jump-page" type="number" min={1} max={catalog.total_pages} value={jumpPage} placeholder={String(catalogPage)} onChange={event=>setJumpPage(event.target.value)}/>
                                <button type="submit" className="page-btn">Ir</button>
                            </form>
                        </nav>
                    </>}
                    {activeTab === "library" && <div className="library-container">
                        <header className="page-heading"><h2>Biblioteca de jogos</h2><p>{filteredInstalledGames.length} de {installedGames.length} jogos instalados</p></header>
                        {installedGames.some(g => g.is_missing) && (
                            <div className="library-missing-banner" role="alert">
                                <div className="banner-text">
                                    <strong>{installedGames.filter(g => g.is_missing).length} {installedGames.filter(g => g.is_missing).length === 1 ? "jogo está ausente" : "jogos estão ausentes"} na pasta de ROMs.</strong>
                                    <span>Arquivos movidos ou deletados não podem ser iniciados no PCSX2 até nova sincronização.</span>
                                </div>
                                <button type="button" className="button-secondary" onClick={() => setLibraryFilterStatus("missing")}>
                                    Ver ausentes
                                </button>
                            </div>
                        )}
                        <div className="library-emulator-bar">
                            <div className="library-emulator-selectors">
                                <div className="library-emulator-field">
                                    <span className="library-emulator-label">
                                        Versão do Emulador:
                                    </span>
                                    <select
                                        value={pcsx2Profiles.find(p => p.is_active)?.executable_path || configs.pcsx2_executable_path || ""}
                                        onChange={e => void handleSwitchPcsx2Version(e.target.value)}
                                        className="library-emulator-select"
                                    >
                                        {pcsx2Installations.map(inst => (
                                            <option key={inst.path} value={inst.path}>
                                                {formatPcsx2Version(inst.version)} ({inst.is_managed ? "Gerenciado" : inst.is_portable ? "Portátil" : "Sistema"})
                                            </option>
                                        ))}
                                        {(pcsx2Profiles.find(p => p.is_active)?.executable_path || configs.pcsx2_executable_path) &&
                                         !pcsx2Installations.some(i => i.path.toLowerCase() === (pcsx2Profiles.find(p => p.is_active)?.executable_path || configs.pcsx2_executable_path).toLowerCase()) && (
                                            <option value={pcsx2Profiles.find(p => p.is_active)?.executable_path || configs.pcsx2_executable_path}>
                                                Personalizado ({pcsx2Profiles.find(p => p.is_active)?.executable_path || configs.pcsx2_executable_path})
                                            </option>
                                        )}
                                    </select>
                                </div>
                                {pcsx2Profiles.length > 0 && (
                                    <div className="library-emulator-field">
                                        <span className="library-emulator-label">
                                            Perfil Ativo:
                                        </span>
                                        <select
                                            value={pcsx2Profiles.find(p => p.is_active)?.id || ""}
                                            onChange={e => void handleSwitchPcsx2Profile(e.target.value)}
                                            className="library-emulator-select"
                                        >
                                            {pcsx2Profiles.map(p => (
                                                <option key={p.id} value={p.id}>
                                                    {p.name} ({p.renderer}, {p.aspect_ratio})
                                                </option>
                                            ))}
                                        </select>
                                    </div>
                                )}
                            </div>
                            <button
                                type="button"
                                onClick={() => setActiveTab("emulator")}
                                className="library-emulator-manage-btn"
                            >
                                Gerenciar Versões e Perfis →
                            </button>
                        </div>
                        <div className="library-controls-group library-toolbar-v2"><label className="library-search-label"><span className="visually-hidden">Buscar na biblioteca</span><input className="compact-input" type="search" placeholder="Buscar na biblioteca…" value={librarySearch} onChange={event=>setLibrarySearch(event.target.value)}/></label><div className="view-mode-buttons"><button type="button" className={`view-mode-btn${viewMode==="grid"?" active":""}`} aria-pressed={viewMode==="grid"} onClick={()=>setViewMode("grid")}>Grade</button><button type="button" className={`view-mode-btn${viewMode==="list"?" active":""}`} aria-pressed={viewMode==="list"} onClick={()=>setViewMode("list")}>Lista</button></div><button type="button" className="button-secondary" disabled={isScanning || !!coverJob?.running} onClick={()=>void scanGames()}>{isScanning?"Sincronizando…":"Sincronizar pasta"}</button><button type="button" className="button-secondary" disabled={isScanning || !!coverJob?.running || !installedGames.length} onClick={()=>void regenerateCovers()}>{coverJob?.running?"Regenerando…":"Regenerar capas"}</button></div>
                        {coverJob?.running && <div className="cover-job" role="status"><div><strong>Regenerando capas: {coverJob.processed} de {coverJob.total}</strong><span>{coverJob.current || "Preparando…"}</span></div><progress aria-label="Progresso da regeneração de capas" max={coverJob.total || 1} value={coverJob.processed}/></div>}
                        <div className="library-filters-bar"><label className="filter-item">Ordenar por<select value={librarySort} onChange={event=>setLibrarySort(event.target.value)}><option value="name_asc">Nome (A–Z)</option><option value="name_desc">Nome (Z–A)</option><option value="size_desc">Maior tamanho</option><option value="size_asc">Menor tamanho</option></select></label><label className="filter-item">Status<select value={libraryFilterStatus} onChange={event=>setLibraryFilterStatus(event.target.value as "all"|"available"|"missing")}><option value="all">Todos</option><option value="available">Disponíveis</option><option value="missing">Ausentes</option></select></label><label className="filter-item">Região<select value={libraryFilterRegion} onChange={event=>setLibraryFilterRegion(event.target.value)}><option value="all">Todas</option>{Array.from(new Set(installedGames.map(game=>game.region).filter((value):value is string=>!!value))).sort().map(region=><option key={region}>{region}</option>)}</select></label><label className="filter-item">Formato<select value={libraryFilterFormat} onChange={event=>setLibraryFilterFormat(event.target.value)}><option value="all">Todos</option>{Array.from(new Set(installedGames.map(game=>game.format.toLowerCase()))).sort().map(format=><option value={format} key={format}>{format.toUpperCase()}</option>)}</select></label>{(librarySearch || libraryFilterRegion!=="all" || libraryFilterFormat!=="all" || libraryFilterStatus!=="all") && <button type="button" className="button-quiet" onClick={()=>{setLibrarySearch("");setLibraryFilterRegion("all");setLibraryFilterFormat("all");setLibraryFilterStatus("all");}}>Limpar filtros</button>}</div>
                        {!filteredInstalledGames.length?<div className="empty-state"><h3>Nenhum jogo encontrado</h3><p>{installedGames.length?"Nenhum resultado para os filtros aplicados.":"Configure a pasta de ROMs e clique em Sincronizar pasta."}</p></div>:<div className={viewMode==="grid"?"library-grid":"library-list"}>{filteredInstalledGames.map(game=><LibraryGameCard key={game.id} game={game} onOpen={()=>openInstalledGame(game)} onLaunch={()=>void launchGame(game.file_path)} onDelete={()=>setGameToDelete({id:game.id,title:game.title,filePath:game.file_path,coverUrl:game.cover_url || undefined})}/>)}</div>}
                    </div>}
                    {activeTab === "search" && <>
                        <header className="page-heading search-page-header">
                            <h2>Pesquisar jogos</h2>
                            {searchResults.length>0 && (
                                <div className="panel-toolbar">
                                    {!isBulkSelectActive ? (
                                        <button type="button" className="button-secondary" onClick={()=>setIsBulkSelectActive(true)}>Selecionar múltiplos</button>
                                    ) : (
                                        <>
                                            <button type="button" className="button-secondary" onClick={()=>selectAllCurrentList(searchResults)}>Selecionar resultados</button>
                                            {selectedBulk.size>0 && <button type="button" className="button-quiet" onClick={clearBulkSelection}>Limpar seleção ({selectedBulk.size})</button>}
                                            <button type="button" className="button-quiet" onClick={()=>{setIsBulkSelectActive(false);clearBulkSelection();}}>Cancelar seleção</button>
                                        </>
                                    )}
                                </div>
                            )}
                        </header>
                        <section className="panel search-panel-v2"><label>Nome do jogo<input className="compact-input" type="search" placeholder="Título ou parte do nome…" value={searchQuery} onChange={event=>setSearchQuery(event.target.value)}/></label><div className="search-options-v2"><label>Termo adicional<input value={searchGenre} onChange={event=>setSearchGenre(event.target.value)} placeholder="Tradução, Undub…"/></label><label>Fonte<select value={searchSource} onChange={event=>setSearchSource(event.target.value)}><option value="all">Todas as fontes (Unificado)</option><option value="catalog">Catálogo local (CHD)</option><option value="cdromance">CDRomance (Traduções / Mods)</option></select></label><label>Região<select value={searchLang} disabled={searchSource==="cdromance"} onChange={event=>setSearchLang(event.target.value)}><option value="all">Todas</option><option value="USA">USA</option><option value="Europe">Europa</option><option value="Japan">Japão</option></select></label><label>Formato<select value={searchFormat} disabled={searchSource==="cdromance"} onChange={event=>setSearchFormat(event.target.value)}><option value="all">Todos</option><option value="chd">CHD</option><option value="iso">ISO</option><option value="7z">7Z</option></select></label><label>Ordem<select value={searchSort} disabled={searchSource==="cdromance"} onChange={event=>setSearchSort(event.target.value)}><option value="relevance">Relevância</option><option value="title_asc">Nome (A–Z)</option><option value="size_desc">Maior tamanho</option></select></label><label>Resultados<select value={searchPerPage} disabled={searchSource==="cdromance"} onChange={event=>setSearchPerPage(Number(event.target.value))}>{[12,24,48].map(value=><option key={value}>{value}</option>)}</select></label></div></section>
                        {isSearching?<div className="loader-state" role="status">Pesquisando…</div>:!searchResults.length?<div className="empty-state"><h3>{searchQuery || searchGenre?"Nenhum título encontrado":"Pesquisa pronta"}</h3><p>{searchQuery || searchGenre?"Nenhum jogo corresponde aos termos e filtros aplicados.":"Digite o título acima para consultar as fontes disponíveis."}</p>{(searchQuery || searchGenre || searchLang!=="all" || searchFormat!=="all" || searchSource!=="all") && <button type="button" className="button-secondary" style={{marginTop:"0.75rem"}} onClick={()=>{setSearchQuery("");setSearchGenre("");setSearchLang("all");setSearchFormat("all");setSearchSource("all");}}>Limpar busca e filtros</button>}</div>:<div className="mods-grid">{searchResults.map(game=>{
                            const installed=isItemInstalled(game);
                            const inQueue=isItemDownloadingOrQueued(game);
                            const disabled=installed || inQueue;
                            const isSelected=selectedBulk.has(game.id);
                            return <article className={`mod-card${isSelected?" selected":""}`} key={game.id}>
                                <div className="mod-thumb-wrap">
                                    {isBulkSelectActive && (
                                        <button
                                            type="button"
                                            className={`card-select-btn${disabled?" disabled":""}${isSelected?" checked":""}`}
                                            disabled={disabled}
                                            onClick={(e)=>{e.stopPropagation();toggleBulkSelect(game);}}
                                            title={installed?"Já instalado na biblioteca":inQueue?"Já na fila ou em download":isSelected?"Desmarcar":"Selecionar para download"}
                                            aria-label={`Selecionar ${game.title}`}
                                            aria-pressed={isSelected}
                                        >
                                            {isSelected && (
                                                <svg viewBox="0 0 14 14" width="13" height="13" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" strokeLinejoin="round">
                                                    <polyline points="2.5 7.5 5.5 10.5 11.5 3.5" />
                                                </svg>
                                            )}
                                        </button>
                                    )}
                                    <button type="button" className="catalog-cover-button mod-thumb" onClick={()=>isBulkSelectActive ? toggleBulkSelect(game) : openGameDetail(game)} aria-label={`Detalhes de ${game.title}`}><CoverImage src={game.cover_url} title={game.title} region={game.region}/>{installed && <span className="mod-badge-installed">Instalado</span>}{!installed && inQueue && <span className="mod-badge-queued">Na fila</span>}</button>
                                </div>
                                <div className="mod-meta">
                                    <h3 className="mod-title"><button type="button" className="card-open" onClick={()=>openGameDetail(game)}>{game.title}</button></h3>
                                    <div className="card-badges-row">
                                        <span className="card-badge-chip card-badge-format">{game.format || (game.filename?.toLowerCase().endsWith(".7z") ? "7Z" : game.filename?.toLowerCase().endsWith(".iso") ? "ISO" : "CHD")}</span>
                                        <span className="card-badge-chip card-badge-region">{game.region || "Livre"}</span>
                                    </div>
                                    <p className="mod-author">{game.size>0 ? formatBytes(game.size) : "Tamanho sob consulta"}</p>
                                    <div className="card-action-bar">
                                        {installed ? (
                                            <button type="button" className="button-secondary" onClick={()=>{setLibrarySearch(game.title);setActiveTab("library");}}>Ver na biblioteca</button>
                                        ) : inQueue ? (
                                            <button type="button" className="button-secondary" onClick={()=>setActiveTab("downloads")}>Ver na fila</button>
                                        ) : (
                                            <button type="button" className="button-secondary" disabled={isStartingDownload || disabled} onClick={()=>void startDownload(game.title,game.url,game.cover_url,game.size)}>Baixar</button>
                                        )}
                                    </div>
                                </div>
                            </article>;
                        })}</div>}
                    </>}
                    {activeTab === "downloads" && <div className="downloads-container">
                        <header className="page-heading downloads-page-header">
                            <div>
                                <h2>Downloads</h2>
                                <p>{runningDownloadsList.length} em andamento · {queuedDownloadsList.length} na fila · {historyDownloadsList.length} no histórico</p>
                            </div>
                            <div className="panel-toolbar">
                                {queuePaused ? (
                                    <button type="button" className="button-primary" onClick={()=>void resumeQueue()}>Retomar fila</button>
                                ) : (
                                    <button type="button" className="button-secondary" disabled={queuedDownloadsList.length===0 && runningDownloadsList.length===0} onClick={()=>void pauseQueue()}>Pausar fila</button>
                                )}
                                <button type="button" className="button-quiet danger-text" disabled={queuedDownloadsList.length===0} onClick={()=>void cancelAllQueued()}>Cancelar todos pendentes</button>
                            </div>
                        </header>
                        <section className="panel"><h3>Em andamento ({runningDownloadsList.length})</h3>{!runningDownloadsList.length && <p className="field-hint">Nenhum download ativo no momento.</p>}{runningDownloadsList.map(download=><article className="download-row-v2" key={download.id}><div className="download-info-v2"><h4>{download.game_title}</h4><p className="field-hint">{STATUS_LABEL[download.status] || download.status} · {download.progress.toFixed(1)}% · {formatBytes(download.speed)}/s · {download.connections || 1} conexões</p><progress aria-label={`Progresso de ${download.game_title}`} max={100} value={download.progress}/><p className="field-hint">{formatBytes(download.downloaded_bytes)} de {formatBytes(download.size)}</p></div>{download.status.toLowerCase()==="downloading" && <button type="button" className="button-quiet danger-text" onClick={()=>void cancelDownload(download.id)}>Cancelar</button>}</article>)}</section>
                        <section className="panel">
                            <div className="panel-header">
                                <h3>Na fila ({queuedDownloadsList.length})</h3>
                                {queuePaused && <span className="queue-paused-badge">Fila pausada</span>}
                            </div>
                            {!queuedDownloadsList.length && <p className="field-hint">Nenhum item aguardando na fila.</p>}
                            {queuedDownloadsList.map((download, index)=><article className="download-row-v2" key={download.id}>
                                <div className="download-info-v2">
                                    <div className="download-queue-title">
                                        <span className="queue-position-badge">#{index + 1}</span>
                                        <h4>{download.game_title}</h4>
                                    </div>
                                    <p className="field-hint">{queuePaused ? "Fila pausada" : "Aguardando vaga"} · {formatBytes(download.size)}</p>
                                </div>
                                <button type="button" className="button-quiet danger-text" onClick={()=>void cancelDownload(download.id)}>Cancelar</button>
                            </article>)}
                        </section>
                        <section className="panel download-history-v2"><div className="panel-header"><h3>Histórico de downloads</h3><button type="button" className="button-quiet" disabled={!historyDownloadsList.length} onClick={()=>void clearDownloadHistory()}>Limpar histórico</button></div>{!historyDownloadsList.length && <p className="field-hint">Nenhum download finalizado.</p>}{historyDownloadsList.map(download=><article className="download-row-v2" key={download.id}><div className="download-info-v2"><h4>{download.game_title}</h4><p className="field-hint">{STATUS_LABEL[download.status] || download.status} · {formatBytes(download.size)}</p>{download.target_path && <p className="download-target-path" title={download.target_path}>{download.target_path}</p>}{download.error_message && <p className="download-error">{download.error_message}</p>}</div><div className="panel-toolbar">{download.status.toLowerCase()==="completed" && <button type="button" className="button-secondary" onClick={()=>{setLibrarySearch(download.game_title);setActiveTab("library");}}>Abrir na biblioteca</button>}{download.status.toLowerCase()==="completed" && download.target_path && <button type="button" className="button-secondary" onClick={()=>void launchGame(download.target_path!)}>Iniciar PCSX2</button>}{["error","cancelled"].includes(download.status.toLowerCase()) && <button type="button" className="button-secondary" onClick={()=>void retryDownload(download.id)}>Tentar novamente</button>}<button type="button" className="button-quiet" onClick={()=>void removeDownload(download.id)}>Remover</button></div></article>)}</section>
                    </div>}
                    {activeTab === "activity" && <ActivityPage jobs={jobs} onRefresh={fetchJobs} />}
                        {activeTab === "settings" && <SettingsPage configs={configs} onChange={(key,value)=>setConfigs(current=>({...current,[key]:value}))}
                        busy={isSavingConfig} validations={[romsValidation,pcsx2Validation,coversValidation]}
                        onSaveRoms={handleSaveRomsDir} onSaveEmulator={handleSavePcsx2Exe} onSaveCovers={handleSaveCoversDir}
                        onSaveAutoCovers={saveAutoCovers} onSaveMaxConcurrent={saveMaxConcurrent} onOpenFolder={openFolder} onRegenerate={regenerateCovers} covers={coverJob}
                        logs={logs} onRefreshLogs={()=>void fetchLogs()} onClearLogs={clearLogs} version={packageInfo.version} onOpenWizard={handleOpenWizard} />}
                    {activeTab === "profile" && <ProfilePage installed={installedGames} favorites={favorites} history={history}
                        onOpenGame={openInstalledGame} onOpenFavorite={favorite=>openGameDetail({...favorite,filename:"",cover_url:favorite.cover_url || "",size:favorite.size || 0,region:favorite.region || "",part:""})}
                        onRemoveFavorite={async favorite=>{await apiFetch(`/api/favorites/${encodeURIComponent(favorite.id)}`,{method:"DELETE"});await fetchFavorites();}}
                        onRefresh={async()=>{await Promise.all([fetchInstalledGames(),fetchFavorites(),fetchHistory()]);}} />}
                    {activeTab === "emulator" && <EmulatorPage onShowToast={showToast} />}
                    {activeTab === "bios" && <BiosPage onShowToast={showToast} />}

                    {activeTab === "game-page" && selectedGame && (
                        <div className="mod-page-view">
                            <div className="mod-page-topbar">
                                <button className="back-btn" onClick={() => setActiveTab(previousTab)}>
                                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.75" strokeLinecap="round" strokeLinejoin="round">
                                        <path d="M19 12H5M12 19l-7-7 7-7" />
                                    </svg>
                                    <span>Voltar</span>
                                </button>
                            </div>

                            <div
                                className="mod-hero-banner"
                                style={{
                                    backgroundImage: `linear-gradient(to bottom, rgba(11, 13, 18, 0.4), rgba(11, 13, 18, 0.95)), url(${selectedGame.cover_url || ""})`
                                }}
                            >
                                <div className="hero-details">
                                    <h1 className="hero-title">{selectedGame.title}</h1>
                                    <p className="hero-author">Região: <strong>{selectedGame.region}</strong> · Formato: <strong>{selectedGame.format || "CHD 1G1R"}</strong></p>
                                    <div className="hero-meta-row">
                                        <div className="hero-meta-stats">
                                            <span className="stat-badge">
                                                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.75" strokeLinecap="round" strokeLinejoin="round">
                                                    <rect x="2" y="6" width="20" height="12" rx="2" />
                                                    <path d="M6 12h4M8 10v4M15 13h.01M18 11h.01" />
                                                </svg>
                                                PlayStation 2
                                            </span>
                                            <span className="stat-badge">
                                                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.75" strokeLinecap="round" strokeLinejoin="round">
                                                    <circle cx="12" cy="12" r="10" />
                                                    <circle cx="12" cy="12" r="3" />
                                                </svg>
                                                {formatBytes(selectedGame.size)}
                                            </span>
                                            {selectedGame.part && (
                                                <span className="stat-badge">
                                                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.75" strokeLinecap="round" strokeLinejoin="round">
                                                        <path d="M20.59 13.41l-7.17 7.17a2 2 0 0 1-2.83 0L2 12V2h10l8.59 8.59a2 2 0 0 1 0 2.82z" />
                                                        <line x1="7" y1="7" x2="7.01" y2="7" />
                                                    </svg>
                                                    {selectedGame.part}
                                                </span>
                                            )}
                                            <span className="stat-badge">
                                                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.75" strokeLinecap="round" strokeLinejoin="round">
                                                    <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" />
                                                    <path d="m9 12 2 2 4-4" />
                                                </svg>
                                                Redump Verificado
                                            </span>
                                        </div>
                                    </div>
                                </div>
                            </div>

                            <div className="mod-layout-grid">
                                <div className="mod-main-column">
                                    <div className="mod-tabs-nav">
                                        <button
                                            className={`mod-tab-btn${detailSubTab === "info" ? " active" : ""}`}
                                            onClick={() => setDetailSubTab("info")}
                                        >
                                            Informações Técnicas
                                        </button>
                                        <button
                                            className={`mod-tab-btn${detailSubTab === "compatibility" ? " active" : ""}`}
                                            onClick={() => setDetailSubTab("compatibility")}
                                        >
                                            Compatibilidade PCSX2
                                        </button>
                                    </div>

                                    <div className="mod-tab-content">
                                        {detailSubTab === "info" && (
                                            <div className="description-panel">
                                                {isLoadingMetadata ? (
                                                    <div style={{ display: "flex", alignItems: "center", gap: "0.75rem", padding: "1.5rem 0", color: "var(--text-muted)" }}>
                                                        <div className="spinner" style={{ width: "20px", height: "20px" }} />
                                                        <span>Carregando sinopse do jogo...</span>
                                                    </div>
                                                ) : gameMetadata?.description ? (
                                                    <div>
                                                        <p style={{ lineHeight: "1.8", color: "var(--text)", fontSize: "0.95rem" }}>
                                                            {gameMetadata.description}
                                                        </p>
                                                        {gameMetadata.source && (
                                                            <div style={{ marginTop: "0.75rem", fontSize: "0.8rem", color: "var(--text-muted)" }}>
                                                                Fonte da sinopse: <strong>{gameMetadata.source}</strong>
                                                            </div>
                                                        )}
                                                    </div>
                                                ) : (
                                                    <p style={{ lineHeight: "1.8", color: "var(--text)" }}>
                                                        <strong>{selectedGame.title}</strong> é um título para PlayStation 2 disponível para emulação no PCSX2.
                                                    </p>
                                                )}

                                                <div style={{ marginTop: "1.5rem", display: "grid", gridTemplateColumns: "1fr 1fr", gap: "1rem" }}>
                                                    <div style={{ background: "var(--bg-2)", padding: "1rem", borderRadius: "8px", border: "1px solid var(--border)" }}>
                                                        <span style={{ fontSize: "0.75rem", color: "var(--text-muted)", display: "block" }}>Tamanho Original</span>
                                                        <strong style={{ fontSize: "1.1rem" }}>{formatBytes(selectedGame.size)}</strong>
                                                    </div>
                                                    <div style={{ background: "var(--bg-2)", padding: "1rem", borderRadius: "8px", border: "1px solid var(--border)" }}>
                                                        <span style={{ fontSize: "0.75rem", color: "var(--text-muted)", display: "block" }}>Região / Padrão de Vídeo</span>
                                                        <strong style={{ fontSize: "1.1rem" }}>{selectedGame.region} ({selectedGame.region === "Europe" ? "PAL 50Hz" : "NTSC 60Hz"})</strong>
                                                    </div>
                                                </div>
                                            </div>
                                        )}

                                        {detailSubTab === "compatibility" && (
                                            <div className="description-panel">
                                                {isLoadingMetadata ? (
                                                    <div style={{ display: "flex", alignItems: "center", gap: "0.75rem", padding: "1.5rem 0", color: "var(--text-muted)" }}>
                                                        <div className="spinner" style={{ width: "20px", height: "20px" }} />
                                                        <span>Consultando lista oficial de compatibilidade do PCSX2...</span>
                                                    </div>
                                                ) : gameMetadata?.compatibility ? (
                                                    <div>
                                                        <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginBottom: "1.25rem", paddingBottom: "1rem", borderBottom: "1px solid var(--border)" }}>
                                                            <div>
                                                                <span style={{ fontSize: "0.8rem", color: "var(--text-muted)", display: "block" }}>Status no Emulador</span>
                                                                <div style={{ marginTop: "0.25rem" }}>
                                                                    {renderCompatBadge(gameMetadata.compatibility.status)}
                                                                </div>
                                                            </div>
                                                            {gameMetadata.compatibility.last_tested_version && (
                                                                <div style={{ textAlign: "right" }}>
                                                                    <span style={{ fontSize: "0.8rem", color: "var(--text-muted)", display: "block" }}>Versão Testada</span>
                                                                    <strong style={{ color: "var(--text)", fontSize: "0.95rem" }}>PCSX2 v{gameMetadata.compatibility.last_tested_version}</strong>
                                                                </div>
                                                            )}
                                                        </div>

                                                        <div style={{ display: "grid", gridTemplateColumns: "repeat(3, 1fr)", gap: "0.75rem", marginBottom: "1.5rem" }}>
                                                            <div style={{ background: "var(--bg-2)", padding: "0.75rem 1rem", borderRadius: "8px", border: "1px solid var(--border)" }}>
                                                                <span style={{ fontSize: "0.75rem", color: "var(--text-muted)", display: "block" }}>Serial Oficial</span>
                                                                <strong style={{ fontSize: "0.95rem" }}>{gameMetadata.compatibility.serial}</strong>
                                                            </div>
                                                            <div style={{ background: "var(--bg-2)", padding: "0.75rem 1rem", borderRadius: "8px", border: "1px solid var(--border)" }}>
                                                                <span style={{ fontSize: "0.75rem", color: "var(--text-muted)", display: "block" }}>Checksum CRC</span>
                                                                <strong style={{ fontSize: "0.95rem" }}>{gameMetadata.compatibility.crc || "N/D"}</strong>
                                                            </div>
                                                            <div style={{ background: "var(--bg-2)", padding: "0.75rem 1rem", borderRadius: "8px", border: "1px solid var(--border)" }}>
                                                                <span style={{ fontSize: "0.75rem", color: "var(--text-muted)", display: "block" }}>Região Oficial</span>
                                                                <strong style={{ fontSize: "0.95rem" }}>{gameMetadata.compatibility.region?.toUpperCase() || selectedGame.region}</strong>
                                                            </div>
                                                        </div>

                                                        <div style={{ display: "flex", gap: "0.75rem", flexWrap: "wrap" }}>
                                                            {gameMetadata.compatibility.wiki_link && (
                                                                <a
                                                                    href={gameMetadata.compatibility.wiki_link}
                                                                    target="_blank"
                                                                    rel="noreferrer"
                                                                    style={{
                                                                        background: "rgba(79, 195, 247, 0.15)",
                                                                        border: "1px solid rgba(79, 195, 247, 0.4)",
                                                                        color: "var(--accent)",
                                                                        borderRadius: "8px",
                                                                        padding: "0.6rem 1.1rem",
                                                                        fontSize: "0.85rem",
                                                                        fontWeight: "600",
                                                                        textDecoration: "none",
                                                                        display: "inline-flex",
                                                                        alignItems: "center",
                                                                        gap: "0.5rem"
                                                                    }}
                                                                >
                                                                    <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.75" strokeLinecap="round" strokeLinejoin="round">
                                                                        <path d="M2 3h6a4 4 0 0 1 4 4v14a3 3 0 0 0-3-3H2z" />
                                                                        <path d="M22 3h-6a4 4 0 0 0-4 4v14a3 3 0 0 1 3-3h7z" />
                                                                    </svg>
                                                                    Ver Guia na PCSX2 Wiki
                                                                </a>
                                                            )}
                                                            {gameMetadata.compatibility.forum_link && (
                                                                <a
                                                                    href={gameMetadata.compatibility.forum_link.replace("&amp;", "&")}
                                                                    target="_blank"
                                                                    rel="noreferrer"
                                                                    style={{
                                                                        background: "var(--bg-3)",
                                                                        border: "1px solid var(--border)",
                                                                        color: "var(--text)",
                                                                        borderRadius: "8px",
                                                                        padding: "0.6rem 1.1rem",
                                                                        fontSize: "0.85rem",
                                                                        fontWeight: "600",
                                                                        textDecoration: "none",
                                                                        display: "inline-flex",
                                                                        alignItems: "center",
                                                                        gap: "0.5rem"
                                                                    }}
                                                                >
                                                                    <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.75" strokeLinecap="round" strokeLinejoin="round">
                                                                        <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" />
                                                                    </svg>
                                                                    Tópico de Testes no Fórum
                                                                </a>
                                                            )}
                                                        </div>
                                                    </div>
                                                ) : (
                                                    <div>
                                                        <p style={{ lineHeight: "1.8", color: "var(--text)" }}>
                                                            Este jogo não possui relatório direto na lista consolidada do PCSX2, mas títulos de PS2 costumam rodar com suporte nativo nas versões 1.7+ e 2.0+ do emulador.
                                                        </p>
                                                        <div style={{ marginTop: "1rem" }}>
                                                            <a
                                                                href={`https://wiki.pcsx2.net/index.php?search=${encodeURIComponent(selectedGame.title)}`}
                                                                target="_blank"
                                                                rel="noreferrer"
                                                                className="sync-btn secondary"
                                                                style={{ padding: "0.6rem 1rem", fontSize: "0.85rem", textDecoration: "none", display: "inline-flex", alignItems: "center", gap: "0.4rem" }}
                                                            >
                                                                <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.75" strokeLinecap="round" strokeLinejoin="round">
                                                                    <circle cx="11" cy="11" r="8" />
                                                                    <line x1="21" y1="21" x2="16.65" y2="16.65" />
                                                                </svg>
                                                                Pesquisar na PCSX2 Wiki
                                                            </a>
                                                        </div>
                                                    </div>
                                                )}
                                            </div>
                                        )}
                                    </div>
                                </div>

                                <div className="mod-side-column">
                                    <div style={{ background: "var(--bg-2)", borderRadius: "var(--radius)", padding: "1.25rem", border: "1px solid var(--border)" }}>
                                        <div style={{ position: "relative", width: "100%", height: "240px", marginBottom: "1rem", borderRadius: "8px", overflow: "hidden" }}>
                                            <CoverImage src={selectedGame.cover_url} title={selectedGame.title} region={selectedGame.region} style={{ width: "100%", height: "100%", objectFit: "cover" }} />
                                        </div>
                                        <div style={{ display: "flex", flexDirection: "column", gap: "0.75rem" }}>
                                            {(() => {
                                                const selectedInstalled = installedGames.find(g => g.id === (selectedGame.installedId || selectedGame.id) || g.file_path === selectedGame.installedPath);
                                                const isSelectedMissing = Boolean(selectedInstalled?.is_missing);
                                                if (selectedGame.isInstalled && selectedGame.installedPath) {
                                                    if (isSelectedMissing) {
                                                        return (
                                                            <button
                                                                className="sync-btn"
                                                                style={{ width: "100%", padding: "0.75rem", opacity: 0.6, cursor: "not-allowed" }}
                                                                disabled={true}
                                                                title="O arquivo da ROM não foi localizado na pasta de jogos"
                                                            >
                                                                <svg viewBox="0 0 24 24" fill="currentColor" stroke="none">
                                                                    <polygon points="6 4 20 12 6 20 6 4" />
                                                                </svg>
                                                                <span>Arquivo ausente no disco</span>
                                                            </button>
                                                        );
                                                    }
                                                    return (
                                                        <div style={{ display: "flex", flexDirection: "column", gap: "0.5rem", width: "100%" }}>
                                                            {pcsx2Installations.length > 0 && (
                                                                <div style={{ display: "flex", alignItems: "center", gap: "0.5rem" }}>
                                                                    <span style={{ fontSize: "0.75rem", color: "#9ca3af", whiteSpace: "nowrap" }}>Versão:</span>
                                                                    <select
                                                                        value={pcsx2Profiles.find(p => p.is_active)?.executable_path || configs.pcsx2_executable_path || ""}
                                                                        onChange={e => void handleSwitchPcsx2Version(e.target.value)}
                                                                        style={{
                                                                            width: "100%",
                                                                            padding: "0.3rem 0.5rem",
                                                                            borderRadius: "4px",
                                                                            background: "#0f172a",
                                                                            border: "1px solid rgba(255, 255, 255, 0.15)",
                                                                            color: "#f3f4f6",
                                                                            fontSize: "0.8rem"
                                                                        }}
                                                                    >
                                                                        {pcsx2Installations.map(inst => (
                                                                            <option key={inst.path} value={inst.path}>
                                                                                {formatPcsx2Version(inst.version)} ({inst.is_managed ? "Gerenciado" : inst.is_portable ? "Portátil" : "Sistema"})
                                                                            </option>
                                                                        ))}
                                                                    </select>
                                                                </div>
                                                            )}
                                                            <button
                                                                className="sync-btn"
                                                                style={{ width: "100%", padding: "0.75rem" }}
                                                                onClick={() => launchGame(selectedGame.installedPath!)}
                                                            >
                                                                <svg viewBox="0 0 24 24" fill="currentColor" stroke="none">
                                                                    <polygon points="6 4 20 12 6 20 6 4" />
                                                                </svg>
                                                                <span>Iniciar no PCSX2</span>
                                                            </button>
                                                        </div>
                                                    );
                                                }
                                                return (
                                                    <button
                                                        className="sync-btn"
                                                        style={{ width: "100%", padding: "0.75rem" }}
                                                        disabled={isStartingDownload}
                                                        onClick={() => startDownload(selectedGame.title, selectedGame.url, selectedGame.cover_url, selectedGame.size)}
                                                    >
                                                        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.75" strokeLinecap="round" strokeLinejoin="round">
                                                            <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
                                                            <polyline points="7 10 12 15 17 10" />
                                                            <line x1="12" y1="15" x2="12" y2="3" />
                                                        </svg>
                                                        <span>Baixar .CHD para PCSX2</span>
                                                    </button>
                                                );
                                            })()}

                                            <button
                                                className="sync-btn secondary"
                                                style={{ width: "100%", padding: "0.75rem" }}
                                                onClick={() => toggleFavorite(selectedGame)}
                                            >
                                                <svg viewBox="0 0 24 24" fill={favorites.some(f => f.id === selectedGame.id) ? "currentColor" : "none"} stroke="currentColor" strokeWidth="1.75" strokeLinecap="round" strokeLinejoin="round">
                                                    <path d="M20.84 4.61a5.5 5.5 0 0 0-7.78 0L12 5.67l-1.06-1.06a5.5 5.5 0 0 0-7.78 7.78l1.06 1.06L12 21.23l7.78-7.78 1.06-1.06a5.5 5.5 0 0 0 0-7.78z" />
                                                </svg>
                                                <span>{favorites.some(f => f.id === selectedGame.id) ? "Remover Favorito" : "Adicionar aos Favoritos"}</span>
                                            </button>

                                            {selectedGame.isInstalled && (
                                                <button
                                                    type="button"
                                                    className="sync-btn secondary"
                                                    style={{ width: "100%", padding: "0.75rem" }}
                                                    disabled={isVerifyingIntegrity}
                                                    onClick={() => void checkIntegrity(selectedGame.installedId || selectedGame.id)}
                                                >
                                                    {isVerifyingIntegrity ? "Verificando integridade…" : "Verificar integridade"}
                                                </button>
                                            )}

                                            {integrityResult && (
                                                <div className="integrity-panel">
                                                    <h4>Relatório de integridade física</h4>
                                                    <div className={`integrity-status-badge ${integrityResult.healthy ? "healthy" : "unhealthy"}`}>
                                                        {integrityResult.healthy ? "✓ Íntegro" : "⚠ Atenção"}
                                                    </div>
                                                    <p className="integrity-message">{integrityResult.message}</p>
                                                    <div className="integrity-details">
                                                        <div><span>Status no disco:</span><strong>{integrityResult.exists ? "Presente" : "Ausente"}</strong></div>
                                                        <div><span>Formato válido:</span><strong>{integrityResult.valid_format ? integrityResult.format.toUpperCase() : "Inválido"}</strong></div>
                                                        <div><span>Tamanho conferido:</span><strong>{integrityResult.size_matches ? "Compatível" : "Diferente"}</strong></div>
                                                        {integrityResult.serial && <div><span>Serial extraído:</span><strong>{integrityResult.serial}</strong></div>}
                                                    </div>
                                                </div>
                                            )}

                                            {selectedGame.isInstalled && (
                                                <button
                                                    className="sync-btn danger"
                                                    style={{
                                                        width: "100%",
                                                        padding: "0.75rem",
                                                        background: "rgba(239, 68, 68, 0.12)",
                                                        color: "#f87171",
                                                        border: "1px solid rgba(239, 68, 68, 0.3)",
                                                        cursor: "pointer",
                                                        fontWeight: "600",
                                                        display: "flex",
                                                        alignItems: "center",
                                                        justifyContent: "center",
                                                        gap: "0.5rem"
                                                    }}
                                                    onClick={() => {
                                                        setGameToDelete({
                                                            id: selectedGame.installedId || selectedGame.id,
                                                            title: selectedGame.title,
                                                            filePath: selectedGame.installedPath,
                                                            coverUrl: selectedGame.cover_url
                                                        });
                                                    }}
                                                >
                                                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.75" strokeLinecap="round" strokeLinejoin="round">
                                                        <polyline points="3 6 5 6 21 6" />
                                                        <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
                                                    </svg>
                                                    <span>Deletar Jogo e Arquivos</span>
                                                </button>
                                            )}
                                        </div>
                                    </div>
                                </div>
                            </div>
                        </div>
                    )}

                    {gameToDelete && <DeleteGameDialog game={gameToDelete} busy={isDeletingGame}
                        onClose={() => setGameToDelete(null)} onConfirm={() => void executeDeleteGame()} />}
                </div>
                {selectedBulk.size > 0 && isBulkSelectActive && (activeTab === "catalog" || activeTab === "search") && (
                    <div className="bulk-download-bar" role="region" aria-label="Seleção de download em massa">
                        <div className="bulk-download-summary">
                            <strong>{selectedBulk.size} {selectedBulk.size === 1 ? "jogo selecionado" : "jogos selecionados"}</strong>
                            <span className="bulk-download-size">{formatBytes(bulkSelectedTotalBytes)} no total</span>
                        </div>
                        <div className="bulk-download-actions">
                            <button
                                type="button"
                                className="button-secondary"
                                onClick={() => selectAllCurrentList(activeTab === "catalog" ? catalog.items : searchResults)}
                            >
                                {activeTab === "catalog" ? "Selecionar página" : "Selecionar resultados"}
                            </button>
                            <button type="button" className="button-quiet" onClick={clearBulkSelection}>
                                Limpar seleção
                            </button>
                            <button
                                type="button"
                                className="button-primary"
                                disabled={isStartingDownload}
                                onClick={() => void startBulkDownloads()}
                            >
                                {isStartingDownload ? "Enfileirando…" : `Baixar selecionados (${selectedBulk.size})`}
                            </button>
                        </div>
                    </div>
                )}
            </main>

            {showSetupWizard && (
                <div className="setup-wizard-overlay" role="dialog" aria-modal="true" aria-labelledby="wizard-dialog-title">
                    <div className="setup-wizard-card">
                        <div className="wizard-progress-track" aria-hidden="true">
                            <div className="wizard-progress-fill" style={{ transform: `scaleX(${(wizardStep + 1) / 4})` }} />
                        </div>

                        <div className="wizard-topbar">
                            <nav className="wizard-steps-dots" aria-label="Etapas da configuração inicial">
                                {(["Visão geral", "Pasta de ROMs", "Emulador PCSX2", "Capas e fila"] as const).map((label, i) => (
                                    <button
                                        key={label}
                                        type="button"
                                        className={`wizard-step-pill${i === wizardStep ? " active" : i < wizardStep ? " done" : ""}`}
                                        disabled={wizardIsFinishing}
                                        aria-current={i === wizardStep ? "step" : undefined}
                                        onClick={() => setWizardStep(i)}
                                    >
                                        <span className="wizard-dot" aria-hidden="true">
                                            {i < wizardStep ? (
                                                <svg viewBox="0 0 16 16" width="12" height="12" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                                                    <polyline points="13 4.5 6.5 11 3 7.5" />
                                                </svg>
                                            ) : (i + 1)}
                                        </span>
                                        <span className="wizard-step-name">{label}</span>
                                    </button>
                                ))}
                            </nav>
                            <span className="wizard-step-counter">Etapa {wizardStep + 1} de 4</span>
                        </div>

                        <div key={wizardStep} className="wizard-step-content">
                            {wizardStep === 0 && (
                                <div className="wizard-welcome">
                                    <div className="wizard-brand-header">
                                        <div className="wizard-logo-wrap">
                                            <img src={logoUrl} alt="" width="48" height="48" className="wizard-logo" />
                                        </div>
                                        <div className="wizard-brand-copy">
                                            <div className="wizard-brand-title-row">
                                                <h2 id="wizard-dialog-title" className="wizard-title">RomManager</h2>
                                                <span className="wizard-version-meta">v{packageInfo.version}</span>
                                            </div>
                                            <p className="wizard-subtitle">Gerenciador local de ROMs de PlayStation 2 com acervo 1G1R integrado, fila de downloads em lote e execução direta no PCSX2.</p>
                                        </div>
                                    </div>

                                    <div className="wizard-feature-cards">
                                        <div className="wizard-feature-card">
                                            <span className="wizard-feature-icon" aria-hidden="true">
                                                <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
                                                    <circle cx="12" cy="12" r="9" />
                                                    <circle cx="12" cy="12" r="2.5" />
                                                </svg>
                                            </span>
                                            <div>
                                                <strong>Biblioteca sincronizada por serial</strong>
                                                <p>Leitura local dos arquivos da pasta de jogos com detecção de região, código serial (SLUS, SCUS, SLES) e vínculo automático de capas.</p>
                                            </div>
                                        </div>
                                        <div className="wizard-feature-card">
                                            <span className="wizard-feature-icon" aria-hidden="true">
                                                <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
                                                    <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
                                                    <polyline points="7 10 12 15 17 10" />
                                                    <line x1="12" y1="15" x2="12" y2="3" />
                                                </svg>
                                            </span>
                                            <div>
                                                <strong>Catálogo 1G1R e fila em lote</strong>
                                                <p>Seleção múltipla de títulos do Internet Archive e consulta sob demanda de traduções, com controle de downloads simultâneos.</p>
                                            </div>
                                        </div>
                                        <div className="wizard-feature-card">
                                            <span className="wizard-feature-icon" aria-hidden="true">
                                                <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
                                                    <polygon points="6 4 20 12 6 20 6 4" />
                                                </svg>
                                            </span>
                                            <div>
                                                <strong>Inicialização direta no PCSX2</strong>
                                                <p>Abertura imediata da ROM selecionada no emulador com registro local das sessões recentes no perfil.</p>
                                            </div>
                                        </div>
                                    </div>
                                </div>
                            )}

                            {wizardStep === 1 && (
                                <div className="wizard-section">
                                    <div className="wizard-section-head">
                                        <h2 id="wizard-dialog-title" className="wizard-title">Pasta de ROMs</h2>
                                        <p className="wizard-subtitle">Defina o diretório onde seus jogos de PlayStation 2 estão salvos e onde novos downloads serão instalados.</p>
                                    </div>

                                    <div className="wizard-input-group">
                                        <label htmlFor="wizard-roms-dir">Caminho da pasta de ROMs</label>
                                        <div className="wizard-input-row">
                                            <input
                                                id="wizard-roms-dir"
                                                type="text"
                                                placeholder="D:\Jogos\PS2"
                                                value={wizardRomsDir}
                                                onChange={e => { setWizardRomsDir(e.target.value); setWizardRomsCheck(null); }}
                                                className="wizard-dir-input"
                                            />
                                            <button
                                                type="button"
                                                className="button-secondary"
                                                disabled={!wizardRomsDir.trim()}
                                                onClick={() => void checkWizardPath("roms-dir", wizardRomsDir)}
                                            >
                                                Validar
                                            </button>
                                        </div>
                                        {wizardRomsCheck ? (
                                            <p className={`field-validation ${wizardRomsCheck.valid ? "valid" : "invalid"}`} role="status">
                                                {wizardRomsCheck.valid ? "✓" : "!"} {wizardRomsCheck.message}
                                            </p>
                                        ) : (
                                            <span className="wizard-input-hint">Opcional agora. Você pode definir ou alterar este caminho depois em Configurações.</span>
                                        )}
                                    </div>
                                </div>
                            )}

                            {wizardStep === 2 && (
                                <div className="wizard-section">
                                    <div className="wizard-section-head">
                                        <h2 id="wizard-dialog-title" className="wizard-title">Emulador PCSX2</h2>
                                        <p className="wizard-subtitle">Informe o caminho completo do executável do PCSX2 para iniciar os jogos diretamente pela biblioteca.</p>
                                    </div>

                                    <div className="wizard-input-group">
                                        <label htmlFor="wizard-pcsx2-exe">Executável do PCSX2 (pcsx2-qt.exe ou pcsx2.exe)</label>
                                        <div className="wizard-input-row">
                                            <input
                                                id="wizard-pcsx2-exe"
                                                type="text"
                                                placeholder="C:\Program Files\PCSX2\pcsx2-qt.exe"
                                                value={wizardPcsx2Path}
                                                onChange={e => { setWizardPcsx2Path(e.target.value); setWizardPcsx2Check(null); }}
                                                className="wizard-dir-input"
                                            />
                                            <button
                                                type="button"
                                                className="button-secondary"
                                                disabled={!wizardPcsx2Path.trim()}
                                                onClick={() => void checkWizardPath("pcsx2-exe", wizardPcsx2Path)}
                                            >
                                                Validar
                                            </button>
                                        </div>
                                        {wizardPcsx2Check ? (
                                            <p className={`field-validation ${wizardPcsx2Check.valid ? "valid" : "invalid"}`} role="status">
                                                {wizardPcsx2Check.valid ? "✓" : "!"} {wizardPcsx2Check.message}
                                            </p>
                                        ) : (
                                            <span className="wizard-input-hint">Deixe em branco caso prefira configurar o emulador mais tarde.</span>
                                        )}
                                    </div>
                                </div>
                            )}

                            {wizardStep === 3 && (
                                <div className="wizard-section">
                                    <div className="wizard-section-head">
                                        <h2 id="wizard-dialog-title" className="wizard-title">Capas e fila de downloads</h2>
                                        <p className="wizard-subtitle">Ajuste a busca automática de capas e o limite de transferências simultâneas.</p>
                                    </div>

                                    <div className="wizard-settings-stack">
                                        <div className="wizard-toggle-row">
                                            <div>
                                                <label htmlFor="wizard-auto-covers" className="wizard-row-title">Buscar capas automaticamente</label>
                                                <p>Baixa capas ausentes durante a sincronização da biblioteca usando o serial ou título do jogo.</p>
                                            </div>
                                            <label className="wizard-switch" htmlFor="wizard-auto-covers">
                                                <input
                                                    id="wizard-auto-covers"
                                                    type="checkbox"
                                                    checked={wizardAutoCovers}
                                                    onChange={e => setWizardAutoCovers(e.target.checked)}
                                                />
                                                <span className="wizard-switch-slider" />
                                            </label>
                                        </div>
                                        <div className="wizard-select-row">
                                            <div>
                                                <label htmlFor="wizard-max-concurrent" className="wizard-row-title">Downloads simultâneos na fila</label>
                                                <p>Quantidade de jogos baixados e extraídos ao mesmo tempo na seleção em lote.</p>
                                            </div>
                                            <select
                                                id="wizard-max-concurrent"
                                                value={wizardMaxConcurrent}
                                                onChange={e => setWizardMaxConcurrent(e.target.value)}
                                                className="wizard-select"
                                            >
                                                <option value="1">1 por vez (menor uso)</option>
                                                <option value="2">2 simultâneos (padrão)</option>
                                                <option value="3">3 simultâneos</option>
                                                <option value="4">4 simultâneos</option>
                                                <option value="5">5 simultâneos (rápido)</option>
                                            </select>
                                        </div>
                                    </div>

                                    <div className="wizard-input-group">
                                        <label htmlFor="wizard-covers-dir">Pasta de capas personalizada (opcional)</label>
                                        <div className="wizard-input-row">
                                            <input
                                                id="wizard-covers-dir"
                                                type="text"
                                                placeholder="D:\Jogos\PS2\covers"
                                                value={wizardCoversDir}
                                                onChange={e => { setWizardCoversDir(e.target.value); setWizardCoversCheck(null); }}
                                                className="wizard-dir-input"
                                            />
                                            <button
                                                type="button"
                                                className="button-secondary"
                                                disabled={!wizardCoversDir.trim()}
                                                onClick={() => void checkWizardPath("covers-dir", wizardCoversDir)}
                                            >
                                                Validar
                                            </button>
                                        </div>
                                        {wizardCoversCheck ? (
                                            <p className={`field-validation ${wizardCoversCheck.valid ? "valid" : "invalid"}`} role="status">
                                                {wizardCoversCheck.valid ? "✓" : "!"} {wizardCoversCheck.message}
                                            </p>
                                        ) : (
                                            <span className="wizard-input-hint">Deixe em branco para usar a pasta padrão gerenciada pelo aplicativo.</span>
                                        )}
                                    </div>
                                </div>
                            )}
                        </div>

                        <div className="wizard-footer">
                            <p className="wizard-footer-note">Alterável a qualquer momento em <strong>Configurações</strong></p>
                            <div className="wizard-footer-actions">
                                <button type="button" className="wizard-skip-all-btn" disabled={wizardIsFinishing} onClick={handleWizardSkipAll}>
                                    Configurar depois
                                </button>
                                {wizardStep > 0 && (
                                    <button
                                        type="button"
                                        className="wizard-skip-all-btn"
                                        disabled={wizardIsFinishing}
                                        onClick={() => setWizardStep(prev => Math.max(0, prev - 1))}
                                    >
                                        Voltar
                                    </button>
                                )}
                                <button
                                    type="button"
                                    className="wizard-next-btn"
                                    onClick={() => void handleWizardNext()}
                                    disabled={wizardIsFinishing}
                                >
                                    {wizardIsFinishing ? "Salvando…" : wizardStep === 0 ? "Começar configuração" : wizardStep === 3 ? "Salvar e concluir" : "Continuar"}
                                </button>
                            </div>
                        </div>
                    </div>
                </div>
            )}
        </div>
    );
}