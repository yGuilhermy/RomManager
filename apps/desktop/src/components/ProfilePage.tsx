import { useState, useEffect, useCallback, useRef } from "react";
import type { FormEvent } from "react";
import type { InstalledGame, FavoriteItem, HistoryItem, ProfileState, LocalProfile, ProfileBackup } from "../types";
import { apiJson } from "../lib/api";
import { formatBytes, formatDate } from "../lib/format";
import { parseProfileBackup, PROFILE_COLORS } from "../lib/profileBackup";
import { CoverImage } from "./CoverImage";
import { showToast } from "./Toast";
import { saveGeneratedFile } from "../lib/nativeFiles";

interface Props {
    installed: InstalledGame[];
    favorites: FavoriteItem[];
    history: HistoryItem[];
    onOpenGame: (game: InstalledGame) => void;
    onOpenFavorite: (favorite: FavoriteItem) => void;
    onRemoveFavorite: (favorite: FavoriteItem) => Promise<void>;
    onRefresh: () => Promise<void>;
}

const tabs = ["collection", "favorites", "activity"] as const;
type ProfileTab = typeof tabs[number];
const tabNames = { collection: "Minha coleção", favorites: "Favoritos", activity: "Atividade" };

export function ProfilePage(props: Props) {
    const [data, setData] = useState<ProfileState | null>(null);
    const [draft, setDraft] = useState<LocalProfile | null>(null);
    const [error, setError] = useState("");
    const [loading, setLoading] = useState(true);
    const [editing, setEditing] = useState(false);
    const [busy, setBusy] = useState(false);
    const [tab, setTab] = useState<ProfileTab>("collection");
    const [query, setQuery] = useState("");
    const [page, setPage] = useState(1);
    const [pendingBackup, setPendingBackup] = useState<ProfileBackup | null>(null);
    const [removing, setRemoving] = useState<string | null>(null);
    const [avatarUrl, setAvatarUrl] = useState<string>(() => {
        try {
            return localStorage.getItem("rom_profile_avatar") || "";
        } catch {
            return "";
        }
    });

    const fileInput = useRef<HTMLInputElement>(null);
    const avatarInputRef = useRef<HTMLInputElement>(null);

    const load = useCallback(async (signal?: AbortSignal) => {
        setLoading(true);
        setError("");
        try {
            let result = await apiJson<ProfileState>("/api/profile", { signal });
            try {
                const legacy = localStorage.getItem("rom_user_name");
                if (legacy && result.profile.name === "Jogador PlayStation" && !localStorage.getItem("rom_profile_migrated")) {
                    const profile = await apiJson<LocalProfile>("/api/profile", {
                        method: "POST",
                        headers: { "Content-Type": "application/json" },
                        body: JSON.stringify({ ...result.profile, name: legacy.trim().slice(0, 60) || result.profile.name }),
                        signal
                    });
                    result = { ...result, profile };
                    localStorage.setItem("rom_profile_migrated", "1");
                }
            } catch {}
            if (!signal?.aborted) {
                setData(result);
                setDraft(result.profile);
            }
        } catch (e) {
            if (!signal?.aborted) setError(e instanceof Error ? e.message : "Falha ao carregar perfil");
        } finally {
            if (!signal?.aborted) setLoading(false);
        }
    }, []);

    useEffect(() => {
        const controller = new AbortController();
        void load(controller.signal);
        return () => controller.abort();
    }, [load]);

    useEffect(() => setPage(1), [query, tab]);

    const refresh = async () => {
        await props.onRefresh();
        await load();
    };

    function handleAvatarUpload(file?: File) {
        if (!file) return;
        if (!file.type.startsWith("image/")) {
            showToast("Selecione um arquivo de imagem válido.", "error");
            return;
        }
        const reader = new FileReader();
        reader.onload = e => {
            const img = new Image();
            img.onload = () => {
                const canvas = document.createElement("canvas");
                const maxDim = 256;
                let w = img.width;
                let h = img.height;
                if (w > h) {
                    if (w > maxDim) {
                        h = Math.round((h * maxDim) / w);
                        w = maxDim;
                    }
                } else {
                    if (h > maxDim) {
                        w = Math.round((w * maxDim) / h);
                        h = maxDim;
                    }
                }
                canvas.width = w;
                canvas.height = h;
                const ctx = canvas.getContext("2d");
                if (ctx) {
                    ctx.drawImage(img, 0, 0, w, h);
                    const dataUrl = canvas.toDataURL("image/jpeg", 0.85);
                    setAvatarUrl(dataUrl);
                    try {
                        localStorage.setItem("rom_profile_avatar", dataUrl);
                        showToast("Foto de perfil atualizada.", "success");
                    } catch {
                        showToast("Erro ao salvar foto de perfil.", "error");
                    }
                }
            };
            img.src = e.target?.result as string;
        };
        reader.readAsDataURL(file);
    }

    function removeAvatar() {
        setAvatarUrl("");
        try {
            localStorage.removeItem("rom_profile_avatar");
            showToast("Foto de perfil removida.", "info");
        } catch {}
    }

    async function saveProfile(event: FormEvent) {
        event.preventDefault();
        if (!draft) return;
        setBusy(true);
        try {
            const profile = await apiJson<LocalProfile>("/api/profile", {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify(draft)
            });
            setData(current => current ? { ...current, profile } : current);
            setDraft(profile);
            setEditing(false);
            showToast("Perfil salvo.", "success");
        } catch (e) {
            showToast(e instanceof Error ? e.message : "Não foi possível salvar o perfil.", "error");
        } finally {
            setBusy(false);
        }
    }

    async function exportBackup() {
        setBusy(true);
        try {
            const backup = await apiJson<ProfileBackup>("/api/profile/export");
            const path = await saveGeneratedFile({ kind: "profile", contents: JSON.stringify(backup) });
            if (path !== null) showToast("Backup salvo.", "success");
        } catch (e) {
            showToast(e instanceof Error ? e.message : "Falha ao exportar backup.", "error");
        } finally {
            setBusy(false);
        }
    }

    async function chooseBackup(file?: File) {
        if (!file) return;
        try {
            if (file.size > 16 * 1024 * 1024) throw new Error("O backup deve ter até 16 MiB.");
            setPendingBackup(parseProfileBackup(JSON.parse((await file.text()).replace(/^\uFEFF/, ""))));
        } catch (e) {
            showToast(e instanceof Error ? e.message : "Arquivo inválido.", "error");
        } finally {
            if (fileInput.current) fileInput.current.value = "";
        }
    }

    async function restore() {
        if (!pendingBackup) return;
        setBusy(true);
        try {
            const state = await apiJson<ProfileState>("/api/profile/import", {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify(pendingBackup)
            });
            setData(state);
            setDraft(state.profile);
            setPendingBackup(null);
            setEditing(false);
            await props.onRefresh();
            showToast("Backup restaurado com sucesso.", "success");
        } catch (e) {
            showToast(e instanceof Error ? e.message : "Falha ao importar backup.", "error");
        } finally {
            setBusy(false);
        }
    }

    async function removeFavorite(favorite: FavoriteItem) {
        setRemoving(favorite.id);
        try {
            await props.onRemoveFavorite(favorite);
            showToast("Favorito removido.", "info");
        } catch (e) {
            showToast(e instanceof Error ? e.message : "Falha ao remover favorito.", "error");
        } finally {
            setRemoving(null);
        }
    }

    if (!data) return <section className="panel profile-v2" aria-busy={loading}><h2>Perfil</h2>{loading ? <p role="status">Carregando perfil…</p> : <><p role="alert">{error || "Não foi possível carregar o perfil."}</p><button type="button" className="button-secondary" onClick={() => void load()}>Tentar novamente</button></>}</section>;

    const profile = data.profile;
    const needle = query.trim().toLocaleLowerCase("pt-BR");
    const games = props.installed.filter(game => game.title.toLocaleLowerCase("pt-BR").includes(needle));
    const favorites = props.favorites.filter(game => game.title.toLocaleLowerCase("pt-BR").includes(needle)).sort((a, b) => a.title.localeCompare(b.title, "pt-BR"));
    const history = [...props.history].filter(item => item.game_name.toLocaleLowerCase("pt-BR").includes(needle)).sort((a, b) => Date.parse(b.timestamp) - Date.parse(a.timestamp));
    const total = tab === "collection" ? games.length : tab === "favorites" ? favorites.length : history.length;
    const pages = Math.max(1, Math.ceil(total / 12));
    const currentPage = Math.min(page, pages);
    const start = (currentPage - 1) * 12;
    const initials = profile.name.trim().split(/\s+/).slice(0, 2).map(word => Array.from(word)[0] || "").join("").toLocaleUpperCase("pt-BR");

    const totalBytes = props.installed.reduce((sum, game) => sum + game.file_size, 0);
    const avgBytes = props.installed.length ? Math.round(totalBytes / props.installed.length) : 0;
    const uniqueRegions = new Set(props.installed.map(game => game.region).filter(Boolean));

    return <div className="profile-v2">
        <header className="profile-v2-header panel">
            <div className={`profile-avatar profile-color-${profile.color}`} aria-hidden="true">
                {avatarUrl ? <img src={avatarUrl} alt="" className="profile-avatar-img" /> : initials}
            </div>
            <div className="profile-v2-identity">
                <p className="field-hint">Membro desde {formatDate(profile.created_at)}</p>
                <h2>{profile.name}</h2>
                {profile.bio && <p>{profile.bio}</p>}
            </div>
            <div className="profile-v2-toolbar">
                <button type="button" className="button-secondary" disabled={busy} onClick={() => { setDraft(profile); setEditing(!editing); }}>
                    {editing ? "Fechar edição" : "Editar perfil"}
                </button>
                <button type="button" className="button-quiet" disabled={busy || loading || editing} onClick={() => void refresh()}>
                    Atualizar
                </button>
            </div>
        </header>

        {error && <p className="load-errors" role="alert">{error}</p>}

        {editing && draft && <form className="panel profile-edit-form" onSubmit={event => void saveProfile(event)}>
            <h3>Editar perfil</h3>
            <label>Nome<input maxLength={60} required disabled={busy} value={draft.name} onChange={event => setDraft({ ...draft, name: event.target.value })} /></label>
            <label>Descrição<textarea rows={3} maxLength={300} disabled={busy} value={draft.bio} onChange={event => setDraft({ ...draft, bio: event.target.value })} /></label>
            <div className="profile-avatar-edit-field">
                <label>Foto de perfil</label>
                <div className="profile-avatar-actions">
                    <button type="button" className="button-secondary" onClick={() => avatarInputRef.current?.click()} disabled={busy}>
                        {avatarUrl ? "Trocar foto" : "Enviar foto"}
                    </button>
                    {avatarUrl && <button type="button" className="button-quiet danger-text" onClick={removeAvatar} disabled={busy}>Remover foto</button>}
                    <input ref={avatarInputRef} type="file" accept="image/*" hidden onChange={event => void handleAvatarUpload(event.target.files?.[0])} />
                </div>
            </div>
            <fieldset disabled={busy}>
                <legend>Cor de fundo alternativa</legend>
                <div className="avatar-color-options">
                    {PROFILE_COLORS.map((color, index) => <label key={color}><input type="radio" name="profile-color" value={color} checked={draft.color === color} onChange={() => setDraft({ ...draft, color })} /><span className={`avatar-color-dot profile-color-${color}`} />{["Azul", "Verde", "Âmbar", "Rosa", "Cinza"][index]}</label>)}
                </div>
            </fieldset>
            <div className="panel-toolbar">
                <button type="submit" className="button-primary" disabled={busy || !draft.name.trim()}>{busy ? "Salvando…" : "Salvar perfil"}</button>
                <button type="button" className="button-quiet" disabled={busy} onClick={() => setEditing(false)}>Cancelar</button>
            </div>
        </form>}

        <dl className="profile-metrics">
            {[
                [props.installed.length, "Jogos na biblioteca"],
                [formatBytes(totalBytes), "Armazenamento total"],
                [props.installed.length ? formatBytes(avgBytes) : "0 B", "Tamanho médio"],
                [props.favorites.length, "Jogos favoritos"],
                [uniqueRegions.size, "Regiões ativas"]
            ].map(([value, label]) => <div key={label}><dt>{label}</dt><dd>{value}</dd></div>)}
        </dl>

        <section className="panel profile-collection">
            <div className="profile-tabs" role="tablist" aria-label="Conteúdo do perfil">
                {tabs.map((name, index) => <button type="button" key={name} id={`profile-tab-${name}`} role="tab" aria-selected={tab === name} tabIndex={tab === name ? 0 : -1} aria-controls="profile-panel" onClick={() => setTab(name)} onKeyDown={event => {
                    if (event.key === "ArrowRight" || event.key === "ArrowLeft") {
                        event.preventDefault();
                        const next = tabs[(index + (event.key === "ArrowRight" ? 1 : 2)) % 3];
                        setTab(next);
                        document.getElementById(`profile-tab-${next}`)?.focus();
                    }
                }}>{tabNames[name]}{name === "favorites" ? ` (${props.favorites.length})` : ""}</button>)}
            </div>

            <div className="profile-filter-row">
                <label className="profile-search-field">
                    <input type="search" value={query} onChange={event => setQuery(event.target.value)} placeholder="Buscar jogo na coleção…" />
                </label>
            </div>

            <div id="profile-panel" role="tabpanel" aria-labelledby={`profile-tab-${tab}`}>
                {total === 0 && <p className="profile-empty-state">
                    {needle ? "Nenhum resultado para esta busca." : tab === "collection" ? "Nenhuma ROM encontrada na biblioteca." : tab === "favorites" ? "Nenhum título adicionado aos favoritos." : "Nenhuma atividade registrada."}
                </p>}

                {tab === "collection" && games.slice(start, start + 12).map(game => <article className="profile-game-row" key={game.id}>
                    <div className="profile-small-cover"><CoverImage src={game.cover_url} title={game.title} /></div>
                    <div className="profile-game-body">
                        <h3><button type="button" className="card-open" onClick={() => props.onOpenGame(game)}>{game.title}</button></h3>
                        <p className="field-hint">{game.region || "Região não informada"} · {formatBytes(game.file_size)} · {game.format.toUpperCase()}{game.serial ? ` · ${game.serial}` : ""}</p>
                    </div>
                </article>)}

                {tab === "favorites" && favorites.slice(start, start + 12).map(favorite => <article className="profile-game-row" key={favorite.id}>
                    <div className="profile-small-cover"><CoverImage src={favorite.cover_url} title={favorite.title} /></div>
                    <div className="profile-game-body">
                        <h3><button type="button" className="card-open" onClick={() => props.onOpenFavorite(favorite)}>{favorite.title}</button></h3>
                        <p className="field-hint">{favorite.region || "Região não informada"} · {formatBytes(favorite.size || 0)}</p>
                    </div>
                    <button type="button" className="button-quiet danger-text" disabled={removing === favorite.id} onClick={() => void removeFavorite(favorite)} aria-label={`Remover ${favorite.title} dos favoritos`}>
                        {removing === favorite.id ? "Removendo…" : "Remover"}
                    </button>
                </article>)}

                {tab === "activity" && <ol className="profile-activity">
                    {history.slice(start, start + 12).map(item => <li key={item.id}>
                        <div>
                            <strong>{item.game_name}</strong>
                            <span>{({ launch: "Iniciado no PCSX2", download: "Download concluído", delete: "ROM excluída", merge_notes: "Notas preservadas na biblioteca" } as Record<string, string>)[item.action] || item.action}</span>
                            {item.details && <p className="field-hint">{item.details}</p>}
                        </div>
                        <time dateTime={item.timestamp}>{formatDate(item.timestamp)}</time>
                    </li>)}
                </ol>}
            </div>

            {pages > 1 && <nav className="profile-pagination" aria-label="Paginação do perfil">
                <button type="button" className="button-quiet" disabled={currentPage === 1} onClick={() => setPage(currentPage - 1)}>Anterior</button>
                <span>{currentPage} de {pages}</span>
                <button type="button" className="button-quiet" disabled={currentPage === pages} onClick={() => setPage(currentPage + 1)}>Próxima</button>
            </nav>}
        </section>

        <section className="panel profile-backups">
            <div>
                <h3>Backup do perfil</h3>
                <p>Exporta ou restaura suas preferências, lista de favoritos e histórico local.</p>
            </div>
            <div className="panel-toolbar">
                <button type="button" className="button-secondary" disabled={busy} onClick={() => void exportBackup()}>Exportar backup</button>
                <button type="button" className="button-quiet" disabled={busy} onClick={() => fileInput.current?.click()}>Importar backup</button>
                <input ref={fileInput} type="file" accept=".json,application/json" hidden onChange={event => void chooseBackup(event.target.files?.[0])} />
            </div>
            {pendingBackup && <div className="backup-preview" role="region" aria-label="Prévia do backup">
                <h4>Importar dados de {pendingBackup.profile.name}</h4>
                <p>{pendingBackup.favorites.length} favoritos e {pendingBackup.history.length} eventos no histórico. Os dados serão mesclados aos existentes.</p>
                <div className="panel-toolbar">
                    <button type="button" className="button-primary" disabled={busy} onClick={() => void restore()}>{busy ? "Importando…" : "Confirmar importação"}</button>
                    <button type="button" className="button-quiet" disabled={busy} onClick={() => setPendingBackup(null)}>Cancelar</button>
                </div>
            </div>}
        </section>
    </div>;
}
