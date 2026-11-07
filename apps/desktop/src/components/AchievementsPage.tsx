import { useState, useEffect, useCallback, useMemo } from "react";
import { apiFetch, apiJson } from "../lib/api";
import type { AchievementsState, RaGameProgress, RaGameSummaryRow } from "../types";

interface AchievementsPageProps {
    embeddedGameId?: string;
    onShowToast?: (message: string, type?: "success" | "error" | "info") => void;
}

type AchievementFilter = "all" | "unlocked" | "pending";

function formatSyncDate(value?: string | null): string {
    if (!value) return "Nunca sincronizado";
    const parsed = new Date(value);
    if (Number.isNaN(parsed.getTime())) return value;
    return parsed.toLocaleString("pt-BR", {
        day: "2-digit",
        month: "2-digit",
        year: "numeric",
        hour: "2-digit",
        minute: "2-digit"
    });
}

function getRecognitionBadge(status: string): { label: string; className: string } {
    switch (status) {
        case "verified":
            return { label: "Verificado por hash", className: "ra-status-chip verified" };
        case "unrecognized":
            return { label: "Mod/tradução sem hash oficial", className: "ra-status-chip warning" };
        case "unsupported_format":
            return { label: "Formato compactado", className: "ra-status-chip neutral" };
        case "missing_file":
            return { label: "Arquivo ausente", className: "ra-status-chip danger" };
        default:
            return { label: "Não verificado", className: "ra-status-chip neutral" };
    }
}

export function AchievementsPage({ embeddedGameId, onShowToast }: AchievementsPageProps) {
    const [state, setState] = useState<AchievementsState | null>(null);
    const [loadingState, setLoadingState] = useState(true);
    const [selectedGameId, setSelectedGameId] = useState<string>(embeddedGameId || "");
    const [gameProgress, setGameProgress] = useState<RaGameProgress | null>(null);
    const [loadingGame, setLoadingGame] = useState(false);
    const [syncingGame, setSyncingGame] = useState(false);
    const [usernameInput, setUsernameInput] = useState("");
    const [secretInput, setSecretInput] = useState("");
    const [submittingAuth, setSubmittingAuth] = useState(false);
    const [authError, setAuthError] = useState<string | null>(null);
    const [gameError, setGameError] = useState<string | null>(null);
    const [gameSearch, setGameSearch] = useState("");
    const [achievementFilter, setAchievementFilter] = useState<AchievementFilter>("all");
    const [achievementSearch, setAchievementSearch] = useState("");

    const loadState = useCallback(async (silent = false) => {
        if (!silent) setLoadingState(true);
        try {
            const data = await apiJson<AchievementsState>("/api/pcsx2/achievements/state");
            setState(data);
            setSelectedGameId(prev => {
                if (embeddedGameId) return embeddedGameId;
                if (prev && data.games.some(g => g.game_id === prev)) return prev;
                return data.games[0]?.game_id || "";
            });
            return data;
        } catch (err) {
            if (!silent) {
                setGameError(err instanceof Error ? err.message : "Falha ao carregar estado do RetroAchievements.");
            }
            return null;
        } finally {
            if (!silent) setLoadingState(false);
        }
    }, [embeddedGameId]);

    const loadGameProgress = useCallback(async (targetGameId: string, forceRefresh = false) => {
        if (!targetGameId) {
            setGameProgress(null);
            return;
        }
        if (forceRefresh) {
            setSyncingGame(true);
        } else {
            setLoadingGame(true);
        }
        setGameError(null);
        try {
            const data = forceRefresh
                ? await apiJson<RaGameProgress>("/api/pcsx2/achievements/game/sync", {
                    method: "POST",
                    headers: { "Content-Type": "application/json" },
                    body: JSON.stringify({ game_id: targetGameId })
                })
                : await apiJson<RaGameProgress>(`/api/pcsx2/achievements/game?game_id=${encodeURIComponent(targetGameId)}`);
            setGameProgress(data);
            if (forceRefresh) {
                await loadState();
                onShowToast?.(
                    data.recognition_status === "verified"
                        ? `Conquistas sincronizadas para ${data.ra_title || data.game_title}.`
                        : data.recognition_message,
                    data.recognition_status === "verified" ? "success" : "info"
                );
            }
        } catch (err) {
            const msg = err instanceof Error ? err.message : "Não foi possível consultar as conquistas deste jogo.";
            setGameError(msg);
            if (forceRefresh) {
                onShowToast?.(msg, "error");
            }
        } finally {
            setLoadingGame(false);
            setSyncingGame(false);
        }
    }, [loadState, onShowToast]);

    useEffect(() => {
        void loadState();
    }, [loadState]);

    useEffect(() => {
        if (!state?.syncing) return;
        const timer = setInterval(async () => {
            const updated = await loadState(true);
            if (updated && selectedGameId) {
                const target = updated.games.find(g => g.game_id === selectedGameId);
                if (target && target.recognition_status !== "not_synced" && gameProgress?.recognition_status === "not_synced") {
                    void loadGameProgress(selectedGameId, false);
                }
            }
        }, 1500);
        return () => clearInterval(timer);
    }, [state?.syncing, loadState, selectedGameId, gameProgress?.recognition_status, loadGameProgress]);

    useEffect(() => {
        if (embeddedGameId) {
            setSelectedGameId(embeddedGameId);
        }
    }, [embeddedGameId]);

    useEffect(() => {
        if (selectedGameId) {
            void loadGameProgress(selectedGameId, false);
        } else {
            setGameProgress(null);
        }
    }, [selectedGameId, loadGameProgress]);

    const handleLogin = async (event: React.FormEvent) => {
        event.preventDefault();
        if (!usernameInput.trim() || !secretInput.trim()) {
            setAuthError("Preencha o usuário e a senha ou token de conexão.");
            return;
        }
        setSubmittingAuth(true);
        setAuthError(null);
        try {
            const updated = await apiJson<AchievementsState>("/api/pcsx2/achievements/login", {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({
                    username: usernameInput.trim(),
                    password: secretInput.trim()
                })
            });
            setState(updated);
            setSecretInput("");
            onShowToast?.("Conta RetroAchievements vinculada no cofre do Windows.", "success");
            if (selectedGameId) {
                await loadGameProgress(selectedGameId, true);
            }
        } catch (err) {
            const msg = err instanceof Error ? err.message : "Falha na autenticação com o RetroAchievements.";
            setAuthError(msg);
            onShowToast?.(msg, "error");
        } finally {
            setSubmittingAuth(false);
        }
    };

    const handleLogout = async () => {
        setSubmittingAuth(true);
        setAuthError(null);
        try {
            await apiFetch("/api/pcsx2/achievements/logout", { method: "POST" });
            setUsernameInput("");
            setSecretInput("");
            await loadState();
            if (selectedGameId) {
                await loadGameProgress(selectedGameId, false);
            }
            onShowToast?.("Conta desvinculada e credencial removida do cofre do sistema.", "info");
        } catch (err) {
            const msg = err instanceof Error ? err.message : "Erro ao desvincular conta.";
            setAuthError(msg);
            onShowToast?.(msg, "error");
        } finally {
            setSubmittingAuth(false);
        }
    };

    const filteredGames = useMemo(() => {
        if (!state?.games) return [];
        const q = gameSearch.trim().toLowerCase();
        if (!q) return state.games;
        return state.games.filter(g =>
            g.game_title.toLowerCase().includes(q) ||
            (g.serial || "").toLowerCase().includes(q) ||
            (g.ra_title || "").toLowerCase().includes(q)
        );
    }, [state?.games, gameSearch]);

    const filteredAchievements = useMemo(() => {
        const list = gameProgress?.achievements || [];
        const q = achievementSearch.trim().toLowerCase();
        return list.filter(item => {
            if (achievementFilter === "unlocked" && !item.unlocked_casual && !item.unlocked_hardcore) {
                return false;
            }
            if (achievementFilter === "pending" && (item.unlocked_casual || item.unlocked_hardcore)) {
                return false;
            }
            if (q && !item.title.toLowerCase().includes(q) && !item.description.toLowerCase().includes(q)) {
                return false;
            }
            return true;
        });
    }, [gameProgress?.achievements, achievementFilter, achievementSearch]);

    const completionPercent = useMemo(() => {
        if (!gameProgress || gameProgress.total_achievements <= 0) return 0;
        return Math.min(100, Math.round((gameProgress.unlocked_casual / gameProgress.total_achievements) * 100));
    }, [gameProgress]);

    if (loadingState && !state) {
        return (
            <div className="loader-state" role="status">
                <div className="spinner" />
                <p>Carregando dados do RetroAchievements…</p>
            </div>
        );
    }

    const isLinked = Boolean(state?.linked && state?.account);
    const account = state?.account || null;
    const pcsx2 = state?.pcsx2 || {
        enabled: false,
        hardcore_mode: false,
        encore_mode: false,
        spectator_mode: false,
        notifications: true,
        sound_effects: true,
        overlays: true,
        pcsx2_username: null
    };

    return (
        <div className={embeddedGameId ? "ra-embedded-container" : "ra-page-container"}>
            {!embeddedGameId && (
                <header className="page-heading">
                    <div>
                        <h2>Conquistas · RetroAchievements</h2>
                        <p>Consulta de progresso por hash de executável PS2 e diagnóstico de integração com o PCSX2</p>
                    </div>
                </header>
            )}

            <div className="ra-top-grid">
                <section className="panel ra-account-panel" aria-label="Conta RetroAchievements">
                    <div className="panel-header">
                        <div>
                            <h3>Conta em consulta</h3>
                            <p className="field-hint">
                                Credencial gravada no Cofre de Credenciais do Windows. Nunca exposta em logs, configurações ou backups de perfil.
                            </p>
                        </div>
                        {isLinked && (
                            <span className="ra-status-chip verified">Vinculada</span>
                        )}
                    </div>

                    {isLinked && account ? (
                        <div className="ra-linked-summary">
                            <div className="ra-metrics-row">
                                <div className="ra-metric-card">
                                    <span className="ra-metric-label">Usuário</span>
                                    <strong className="ra-metric-value">{account.username}</strong>
                                </div>
                                <div className="ra-metric-card">
                                    <span className="ra-metric-label">Pontos Hardcore</span>
                                    <strong className="ra-metric-value tabular-nums">{account.score.toLocaleString("pt-BR")}</strong>
                                </div>
                                <div className="ra-metric-card">
                                    <span className="ra-metric-label">Pontos Softcore</span>
                                    <strong className="ra-metric-value tabular-nums">{account.softcore_score.toLocaleString("pt-BR")}</strong>
                                </div>
                                <div className="ra-metric-card">
                                    <span className="ra-metric-label">Última sincronização</span>
                                    <strong className="ra-metric-value ra-metric-date tabular-nums">{formatSyncDate(account.synced_at)}</strong>
                                </div>
                            </div>
                            <div className="panel-toolbar ra-account-actions">
                                <a
                                    href={account.profile_url}
                                    target="_blank"
                                    rel="noreferrer"
                                    className="button-secondary"
                                >
                                    Abrir perfil oficial
                                </a>
                                <button
                                    type="button"
                                    className="button-quiet danger-text"
                                    disabled={submittingAuth}
                                    onClick={() => void handleLogout()}
                                >
                                    {submittingAuth ? "Desvinculando…" : "Desvincular conta"}
                                </button>
                            </div>
                        </div>
                    ) : (
                        <form className="ra-login-form" onSubmit={event => void handleLogin(event)}>
                            <div className="ra-login-fields">
                                <div className="ra-field-group">
                                    <label htmlFor="ra-username">Usuário RetroAchievements</label>
                                    <input
                                        id="ra-username"
                                        type="text"
                                        className="compact-input"
                                        autoComplete="username"
                                        placeholder="Seu nome de usuário"
                                        value={usernameInput}
                                        onChange={event => setUsernameInput(event.target.value)}
                                    />
                                </div>
                                <div className="ra-field-group">
                                    <label htmlFor="ra-secret">Senha ou token de conexão</label>
                                    <input
                                        id="ra-secret"
                                        type="password"
                                        className="compact-input"
                                        autoComplete="current-password"
                                        placeholder="Senha da conta ou Connect Token"
                                        value={secretInput}
                                        onChange={event => setSecretInput(event.target.value)}
                                    />
                                </div>
                                <div className="ra-login-submit">
                                    <button type="submit" className="button-primary" disabled={submittingAuth}>
                                        {submittingAuth ? "Autenticando…" : "Vincular em consulta"}
                                    </button>
                                </div>
                            </div>
                            {authError && <p className="ra-error-banner" role="alert">{authError}</p>}
                        </form>
                    )}
                </section>

                <section className="panel ra-pcsx2-panel" aria-label="Estado do PCSX2">
                    <div className="panel-header">
                        <div>
                            <h3>Estado no PCSX2 (somente leitura)</h3>
                            <p className="field-hint">
                                Desbloqueios ocorrem exclusivamente dentro do PCSX2 durante a emulação. Não disponível em console PS2 físico via OPL.
                            </p>
                        </div>
                        <span className={pcsx2.enabled ? "ra-status-chip verified" : "ra-status-chip neutral"}>
                            {pcsx2.enabled ? "Ativo no emulador" : "Inativo no emulador"}
                        </span>
                    </div>

                    <div className="ra-metrics-row">
                        <div className="ra-metric-card">
                            <span className="ra-metric-label">Conquistas</span>
                            <strong className="ra-metric-value">{pcsx2.enabled ? "Ativadas" : "Desativadas"}</strong>
                        </div>
                        <div className="ra-metric-card">
                            <span className="ra-metric-label">Modo Hardcore</span>
                            <strong className="ra-metric-value">{pcsx2.hardcore_mode ? "Ativo" : "Inativo"}</strong>
                        </div>
                        <div className="ra-metric-card">
                            <span className="ra-metric-label">Modo Encore</span>
                            <strong className="ra-metric-value">{pcsx2.encore_mode ? "Ativo" : "Inativo"}</strong>
                        </div>
                        <div className="ra-metric-card">
                            <span className="ra-metric-label">Conta no PCSX2</span>
                            <strong className="ra-metric-value">{pcsx2.pcsx2_username || "Não configurada"}</strong>
                        </div>
                    </div>
                </section>
            </div>

            <div className={embeddedGameId ? "ra-single-layout" : "ra-master-detail-layout"}>
                {!embeddedGameId && (
                    <aside className="panel ra-games-sidebar" aria-label="Jogos da biblioteca">
                        <div className="ra-sidebar-header">
                            <div className="ra-sidebar-title-row">
                                <h3>Biblioteca PS2 ({filteredGames.length})</h3>
                                {state?.syncing && (
                                    <div className="ra-bg-badge">
                                        <div className="spinner-inline" />
                                        <span>Calculando hashes ({state.sync_completed || 0}/{state.sync_total || 0})</span>
                                    </div>
                                )}
                            </div>
                            <label htmlFor="ra-game-search" className="visually-hidden">Filtrar jogos da biblioteca</label>
                            <input
                                id="ra-game-search"
                                type="search"
                                className="compact-input"
                                placeholder="Filtrar por título ou serial…"
                                value={gameSearch}
                                onChange={event => setGameSearch(event.target.value)}
                            />
                        </div>
                        {!filteredGames.length ? (
                            <p className="field-hint">Nenhum jogo instalado corresponde ao filtro.</p>
                        ) : (
                            <div className="ra-games-list" role="listbox" aria-label="Selecionar jogo para consultar conquistas">
                                {filteredGames.map((row: RaGameSummaryRow) => {
                                    const active = row.game_id === selectedGameId;
                                    const badge = getRecognitionBadge(row.recognition_status);
                                    return (
                                        <button
                                            key={row.game_id}
                                            type="button"
                                            role="option"
                                            aria-selected={active}
                                            className={`ra-game-row${active ? " active theme-selection" : ""}`}
                                            onClick={() => setSelectedGameId(row.game_id)}
                                        >
                                            <div className="ra-game-row-top">
                                                <strong className="ra-game-row-title">{row.game_title}</strong>
                                                <span className="card-badge-chip card-badge-format">{row.format.toUpperCase()}</span>
                                            </div>
                                            <div className="ra-game-row-meta">
                                                <span>{row.serial || "Sem serial"}</span>
                                                <span className={badge.className}>{badge.label}</span>
                                            </div>
                                            {row.total_achievements > 0 && (
                                                <div className="ra-game-row-progress tabular-nums">
                                                    <span>{row.unlocked_casual}/{row.total_achievements} desbloqueadas</span>
                                                    {row.unlocked_hardcore > 0 && (
                                                        <span>{row.unlocked_hardcore} Hardcore</span>
                                                    )}
                                                </div>
                                            )}
                                        </button>
                                    );
                                })}
                            </div>
                        )}
                    </aside>
                )}

                <section className="panel ra-detail-panel" aria-label="Detalhes de conquistas do jogo">
                    {!selectedGameId ? (
                        <div className="empty-state">
                            <h3>Selecione um jogo da biblioteca</h3>
                            <p>Escolha um título instalado ao lado para verificar o hash do executável PS2 e consultar suas conquistas.</p>
                        </div>
                    ) : loadingGame && !gameProgress ? (
                        <div className="loader-state" role="status">
                            <div className="spinner" />
                            <p>Verificando hash do executável PS2 e consultando cache…</p>
                        </div>
                    ) : gameError && !gameProgress ? (
                        <div className="empty-state">
                            <h3>Falha ao consultar conquistas</h3>
                            <p>{gameError}</p>
                            <button
                                type="button"
                                className="button-secondary"
                                onClick={() => void loadGameProgress(selectedGameId, true)}
                            >
                                Tentar novamente
                            </button>
                        </div>
                    ) : gameProgress ? (
                        <div className="ra-detail-content">
                            <header className="ra-detail-header">
                                <div className="ra-detail-title-group">
                                    <div className="ra-detail-badges">
                                        <span className={getRecognitionBadge(gameProgress.recognition_status).className}>
                                            {getRecognitionBadge(gameProgress.recognition_status).label}
                                        </span>
                                        {gameProgress.serial && (
                                            <span className="card-badge-chip card-badge-region">{gameProgress.serial}</span>
                                        )}
                                        {gameProgress.boot_executable && (
                                            <span className="card-badge-chip card-badge-format">ELF: {gameProgress.boot_executable}</span>
                                        )}
                                        {gameProgress.offline && (
                                            <span className="ra-status-chip warning">Cache local (offline)</span>
                                        )}
                                    </div>
                                    <h3>{gameProgress.ra_title || gameProgress.game_title}</h3>
                                    <p className="field-hint">{gameProgress.recognition_message}</p>
                                    <div className="ra-hash-meta tabular-nums">
                                        <span>MD5 rcheevos: <code>{gameProgress.file_hash || "N/D"}</code></span>
                                        <span>Sincronizado: <strong>{formatSyncDate(gameProgress.synced_at)}</strong></span>
                                    </div>
                                </div>

                                <div className="panel-toolbar ra-detail-actions">
                                    <button
                                        type="button"
                                        className="button-primary"
                                        disabled={syncingGame || gameProgress.recognition_status === "unsupported_format" || gameProgress.recognition_status === "missing_file"}
                                        onClick={() => void loadGameProgress(selectedGameId, true)}
                                    >
                                        {syncingGame ? "Sincronizando…" : "Sincronizar conquistas"}
                                    </button>
                                    {gameProgress.official_game_url && (
                                        <a
                                            href={gameProgress.official_game_url}
                                            target="_blank"
                                            rel="noreferrer"
                                            className="button-secondary"
                                        >
                                            Abrir no RetroAchievements
                                        </a>
                                    )}
                                </div>
                            </header>

                            {gameProgress.recognition_status === "verified" && (
                                <div className="ra-progress-summary">
                                    <div className="ra-metrics-row">
                                        <div className="ra-metric-card">
                                            <span className="ra-metric-label">Progresso Geral</span>
                                            <strong className="ra-metric-value tabular-nums">
                                                {gameProgress.unlocked_casual} / {gameProgress.total_achievements} ({completionPercent}%)
                                            </strong>
                                        </div>
                                        <div className="ra-metric-card">
                                            <span className="ra-metric-label">Desbloqueadas Hardcore</span>
                                            <strong className="ra-metric-value tabular-nums">
                                                {gameProgress.unlocked_hardcore} / {gameProgress.total_achievements}
                                            </strong>
                                        </div>
                                        <div className="ra-metric-card">
                                            <span className="ra-metric-label">Pontos Obtidos</span>
                                            <strong className="ra-metric-value tabular-nums">
                                                {gameProgress.points_earned_casual.toLocaleString("pt-BR")} / {gameProgress.points_total.toLocaleString("pt-BR")} pts
                                            </strong>
                                        </div>
                                        <div className="ra-metric-card">
                                            <span className="ra-metric-label">Pontos Hardcore</span>
                                            <strong className="ra-metric-value tabular-nums">
                                                {gameProgress.points_earned_hardcore.toLocaleString("pt-BR")} pts
                                            </strong>
                                        </div>
                                    </div>
                                    <progress
                                        className="ra-progress-bar"
                                        aria-label={`Progresso de conquistas de ${gameProgress.game_title}`}
                                        max={gameProgress.total_achievements || 1}
                                        value={gameProgress.unlocked_casual}
                                    />
                                </div>
                            )}

                            {gameProgress.total_achievements > 0 ? (
                                <>
                                    <div className="ra-achievements-toolbar">
                                        <div className="sort-tabs" role="tablist" aria-label="Filtro de conquistas">
                                            <button
                                                type="button"
                                                role="tab"
                                                aria-selected={achievementFilter === "all"}
                                                className={`sort-tab-btn${achievementFilter === "all" ? " active theme-selection" : ""}`}
                                                onClick={() => setAchievementFilter("all")}
                                            >
                                                Todas ({gameProgress.total_achievements})
                                            </button>
                                            <button
                                                type="button"
                                                role="tab"
                                                aria-selected={achievementFilter === "unlocked"}
                                                className={`sort-tab-btn${achievementFilter === "unlocked" ? " active theme-selection" : ""}`}
                                                onClick={() => setAchievementFilter("unlocked")}
                                            >
                                                Desbloqueadas ({gameProgress.unlocked_casual})
                                            </button>
                                            <button
                                                type="button"
                                                role="tab"
                                                aria-selected={achievementFilter === "pending"}
                                                className={`sort-tab-btn${achievementFilter === "pending" ? " active theme-selection" : ""}`}
                                                onClick={() => setAchievementFilter("pending")}
                                            >
                                                Pendentes ({Math.max(0, gameProgress.total_achievements - gameProgress.unlocked_casual)})
                                            </button>
                                        </div>

                                        <div className="ra-achievement-search-wrap">
                                            <label htmlFor="ra-achievement-search" className="visually-hidden">Buscar conquista</label>
                                            <input
                                                id="ra-achievement-search"
                                                type="search"
                                                className="compact-input"
                                                placeholder="Buscar conquista por nome ou descrição…"
                                                value={achievementSearch}
                                                onChange={event => setAchievementSearch(event.target.value)}
                                            />
                                        </div>
                                    </div>

                                    {!filteredAchievements.length ? (
                                        <div className="empty-state">
                                            <h3>Nenhuma conquista para este filtro</h3>
                                            <p>Altere o filtro entre Todas, Desbloqueadas ou Pendentes para visualizar outros itens.</p>
                                        </div>
                                    ) : (
                                        <div className="ra-achievements-list">
                                            {filteredAchievements.map(item => {
                                                const isUnlocked = item.unlocked_casual || item.unlocked_hardcore;
                                                return (
                                                    <article
                                                        key={item.id}
                                                        className={`ra-achievement-card${isUnlocked ? " unlocked" : " locked"}`}
                                                    >
                                                        <img
                                                            src={isUnlocked ? item.badge_url : item.badge_locked_url}
                                                            alt={item.title}
                                                            className="ra-achievement-badge"
                                                            loading="lazy"
                                                        />
                                                        <div className="ra-achievement-body">
                                                            <div className="ra-achievement-title-row">
                                                                <h4>{item.title}</h4>
                                                                <div className="ra-achievement-tags">
                                                                    <span className="ra-points-pill tabular-nums">{item.points} pts</span>
                                                                    {item.unlocked_hardcore ? (
                                                                        <span className="ra-status-chip verified">Hardcore</span>
                                                                    ) : item.unlocked_casual ? (
                                                                        <span className="ra-status-chip casual">Casual</span>
                                                                    ) : (
                                                                        <span className="ra-status-chip neutral">Pendente</span>
                                                                    )}
                                                                </div>
                                                            </div>
                                                            <p className="ra-achievement-desc">{item.description}</p>
                                                            <div className="ra-achievement-footer">
                                                                {item.kind && <span>Tipo: {item.kind}</span>}
                                                                <a
                                                                    href={item.official_url}
                                                                    target="_blank"
                                                                    rel="noreferrer"
                                                                    className="ra-official-link"
                                                                >
                                                                    Ver no RetroAchievements
                                                                </a>
                                                            </div>
                                                        </div>
                                                    </article>
                                                );
                                            })}
                                        </div>
                                    )}
                                </>
                            ) : (
                                <div className="empty-state">
                                    <h3>
                                        {gameProgress.recognition_status === "unrecognized"
                                            ? "Executável não possui conjunto oficial no RetroAchievements"
                                            : gameProgress.recognition_status === "unsupported_format"
                                            ? "Verificação direta requer imagem ISO"
                                            : "Conquistas ainda não sincronizadas"}
                                    </h3>
                                    <p>{gameProgress.recognition_message}</p>
                                    {gameProgress.recognition_status === "not_synced" && (
                                        <button
                                            type="button"
                                            className="button-primary"
                                            disabled={syncingGame}
                                            onClick={() => void loadGameProgress(selectedGameId, true)}
                                        >
                                            {syncingGame ? "Consultando…" : "Consultar hash no RetroAchievements"}
                                        </button>
                                    )}
                                </div>
                            )}
                        </div>
                    ) : null}
                </section>
            </div>
        </div>
    );
}
