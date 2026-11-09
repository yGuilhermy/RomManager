import { useEffect, useRef, useState } from "react";
import { apiJson, ApiError } from "../lib/api";
import { formatBytes } from "../lib/format";
import {
  filterConsoleGames,
  profileHint,
  unconfirmedInventory,
  consoleDisplayPath,
  consoleTargetName,
  type ConsoleGame,
  type ConsoleInventory,
  type ConsoleProfile,
  type ConsoleTarget,
  type ConsoleVolume,
} from "../lib/console";
import { BrowsePathButton } from "./BrowsePathButton";
import { CoverImage } from "./CoverImage";
import "./ConsolePage.css";

const profiles: [ConsoleProfile, string][] = [
  ["unknown", "Não sei a versão"],
  ["legacy_fat32", "Legado · FAT32 / UL"],
  ["modern_exfat", "Moderno · exFAT / ISO"],
];
const options = (body: unknown) => ({
  method: "POST",
  headers: { "Content-Type": "application/json" },
  body: JSON.stringify(body),
});
const cache = new Map<string, ConsoleInventory>();
const savedId = () => {
  try {
    return localStorage.getItem("console-target") || "";
  } catch {
    return "";
  }
};
const message = (e: unknown) => (e instanceof Error ? e.message : String(e));

export function ConsolePage() {
  const [targets, setTargets] = useState<ConsoleTarget[]>([]),
    [volumes, setVolumes] = useState<ConsoleVolume[]>([]);
  const [selected, setSelected] = useState(savedId),
    [view, setView] = useState<ConsoleInventory | null>(null);
  const [adding, setAdding] = useState(false),
    [path, setPath] = useState(""),
    [profile, setProfile] = useState<ConsoleProfile>("unknown");
  const [busy, setBusy] = useState(false),
    [listing, setListing] = useState(true),
    [error, setError] = useState("");
  const [query, setQuery] = useState(""),
    [method, setMethod] = useState("all"),
    [issuesOnly, setIssuesOnly] = useState(false),
    [layout, setLayout] = useState("grid");
  const [details, setDetails] = useState<ConsoleGame | null>(null),
    [confirmForget, setConfirmForget] = useState(false);
  const [page, setPage] = useState(1);
  const pendingRefresh = useRef(false);
  const detailsDialog = useRef<HTMLDialogElement | null>(null);
  const detailsTrigger = useRef<HTMLElement | null>(null);
  const listingGeneration = useRef(0);
  const generation = useRef(0),
    mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      generation.current++;
      listingGeneration.current++;
    };
  }, []);

  function showDetails(game: ConsoleGame, trigger: HTMLElement) {
    detailsTrigger.current = trigger;
    setDetails(game);
  }
  function closeDetails() {
    detailsDialog.current?.close();
    setDetails(null);
    const trigger = detailsTrigger.current?.isConnected
      ? detailsTrigger.current
      : document.getElementById("console-target");
    trigger?.focus({ preventScroll: true });
  }

  function reportFailure(error: unknown) {
    setError(message(error));
    if (
      error instanceof TypeError ||
      (error instanceof ApiError && error.status >= 500) ||
      (error instanceof Error &&
        ["TimeoutError", "AbortError"].includes(error.name))
    ) {
      setView((current) => (current ? unconfirmedInventory(current) : current));
    }
  }

  async function loadTargets(checkSelected = false) {
    const token = ++listingGeneration.current;
    setListing(true);
    setError("");
    const results = await Promise.allSettled([
      apiJson<ConsoleTarget[]>("/api/console/targets"),
      apiJson<ConsoleVolume[]>("/api/console/volumes"),
    ]);
    if (!mounted.current || token !== listingGeneration.current) return;
    if (results[0].status === "fulfilled") {
      const items = results[0].value;
      setTargets(items);
      setSelected((current) =>
        items.some((t) => t.id === current) ? current : items[0]?.id || "",
      );
      if (!items.length) setAdding(true);
      if (checkSelected && items.some((t) => t.id === selected))
        void read(selected);
    } else reportFailure(results[0].reason);
    if (results[1].status === "fulfilled") setVolumes(results[1].value);
    else reportFailure(results[1].reason);
    setListing(false);
  }
  useEffect(() => {
    void loadTargets();
  }, []);
  async function read(id: string, refresh = false) {
    const token = ++generation.current;
    setBusy(true);
    setError("");
    setDetails(null);
    setConfirmForget(false);
    if (!refresh || view?.target.id !== id) {
      const stored = cache.get(id);
      setView(stored ? unconfirmedInventory(stored) : null);
    }
    try {
      const data = await apiJson<ConsoleInventory>(
        `/api/console/targets/${id}/${refresh ? "refresh" : "inventory"}`,
        refresh ? options({}) : undefined,
      );
      if (!mounted.current || token !== generation.current) return;
      cache.set(id, data);
      setView(data);
      setTargets((items) => items.map((t) => (t.id === id ? data.target : t)));
    } catch (e) {
      if (mounted.current && token === generation.current) {
        setError(message(e));
        setView((current) =>
          current?.target.id === id ? unconfirmedInventory(current) : current,
        );
      }
      if (refresh) {
        try {
          const stored = await apiJson<ConsoleInventory>(
            `/api/console/targets/${id}/inventory`,
          );
          if (mounted.current && token === generation.current) {
            cache.set(id, stored);
            setView(stored);
          }
        } catch {}
      }
    } finally {
      if (mounted.current && token === generation.current) setBusy(false);
    }
  }
  useEffect(() => {
    if (selected) {
      try {
        localStorage.setItem("console-target", selected);
      } catch {}
      const refresh = pendingRefresh.current;
      pendingRefresh.current = false;
      void read(selected, refresh);
    } else {
      generation.current++;
      setView(null);
      setBusy(false);
    }
  }, [selected]);
  async function add(event: React.FormEvent) {
    event.preventDefault();
    setBusy(true);
    setError("");
    try {
      const target = await apiJson<ConsoleTarget>(
        "/api/console/targets",
        options({ path: path.trim().replace(/^"|"$/g, ""), profile }),
      );
      if (!mounted.current) return;
      setTargets((items) => [
        ...items.filter((t) => t.id !== target.id),
        target,
      ]);
      setAdding(false);
      if (target.id === selected) await read(target.id, true);
      else {
        pendingRefresh.current = true;
        setSelected(target.id);
      }
    } catch (e) {
      if (mounted.current) reportFailure(e);
    } finally {
      if (mounted.current) setBusy(false);
    }
  }
  async function changeProfile(value: ConsoleProfile) {
    const id = selected;
    setBusy(true);
    setError("");
    try {
      await apiJson(
        `/api/console/targets/${id}/profile`,
        options({ profile: value }),
      );
      await read(id);
    } catch (e) {
      if (mounted.current) reportFailure(e);
    } finally {
      if (mounted.current) setBusy(false);
    }
  }
  async function forget() {
    setBusy(true);
    setError("");
    const id = selected;
    try {
      await apiJson(`/api/console/targets/${id}`, { method: "DELETE" });
      cache.delete(id);
      setView(null);
      setSelected("");
      setConfirmForget(false);
      await loadTargets();
    } catch (e) {
      if (mounted.current) reportFailure(e);
    } finally {
      if (mounted.current) setBusy(false);
    }
  }
  const games = view?.snapshot?.games || [];
  const filtered = filterConsoleGames(games, query, method, issuesOnly);
  const pageCount = Math.max(1, Math.ceil(filtered.length / 24));
  const currentPage = Math.min(page, pageCount),
    visible = filtered.slice((currentPage - 1) * 24, currentPage * 24);
  useEffect(() => setPage(1), [query, method, issuesOnly, selected]);
  const gamesWithWarnings = games.filter((g) => g.issues.length).length;
  return (
    <section className="console-page" aria-labelledby="console-title">
      <header className="catalog-page-heading">
        <div>
          <div className="field-hint">CONSOLE · OPL</div>
          <h2 id="console-title">Biblioteca do Console</h2>
        </div>
        <span className="field-hint">Somente leitura</span>
      </header>
      <section className="panel console-destination" aria-label="Destino OPL">
        <div className="console-toolbar">
          <label className="console-grow" htmlFor="console-target">
            Pendrive ou pasta
            <select
              aria-label="Pendrive ou pasta"
              id="console-target"
              value={selected}
              disabled={busy || listing}
              onChange={(e) => setSelected(e.target.value)}
            >
              <option value="">Selecione um destino</option>
              {targets.map((t) => (
                <option
                  key={t.id}
                  value={t.id}
                  title={consoleDisplayPath(t.path)}
                >
                  {consoleTargetName(t)}
                </option>
              ))}
            </select>
          </label>
          <button
            type="button"
            className="button-secondary"
            onClick={() => setAdding((v) => !v)}
            disabled={busy || listing}
          >
            {adding && targets.length ? "Fechar" : "Adicionar destino"}
          </button>
          <button
            type="button"
            className="button-secondary"
            onClick={() => void loadTargets(true)}
            disabled={busy || listing}
          >
            {listing ? "Procurando…" : "Procurar unidades"}
          </button>
        </div>
        {adding && (
          <form className="console-add-form" onSubmit={add}>
            <label>
              Unidades disponíveis
              <select
                aria-label="Unidades disponíveis"
                value={volumes.some((v) => v.mount === path) ? path : ""}
                onChange={(e) => setPath(e.target.value)}
              >
                <option value="">Usar outra pasta…</option>
                {volumes.map((v) => (
                  <option value={v.mount} key={v.identity}>
                    {v.mount} · {v.label || "Sem nome"} · {v.filesystem} ·{" "}
                    {formatBytes(v.free_bytes)} livres
                    {v.removable ? " · removível" : ""}
                  </option>
                ))}
              </select>
            </label>
            <label className="console-grow" htmlFor="console-path">
              Pasta OPL
            </label>
            <div className="console-toolbar">
              <input
                id="console-path"
                required
                value={path}
                onChange={(e) => setPath(e.target.value)}
                placeholder="E:\ ou pasta de preparação"
                disabled={busy}
              />
              <BrowsePathButton
                title="Selecionar pendrive ou pasta OPL"
                directory
                current={path}
                onSelect={setPath}
                disabled={busy}
              />
            </div>
            <label>
              Perfil do OPL
              <select
                aria-label="Perfil para novo destino"
                value={profile}
                onChange={(e) => setProfile(e.target.value as ConsoleProfile)}
                disabled={busy}
              >
                {profiles.map(([value, label]) => (
                  <option value={value} key={value}>
                    {label}
                  </option>
                ))}
              </select>
            </label>
            <p className="field-hint">{profileHint(profile)}</p>
            <div>
              <button
                className="button-primary"
                type="submit"
                disabled={busy || listing || !path.trim()}
              >
                {busy ? "Lendo destino…" : "Adicionar e ler jogos"}
              </button>
            </div>
          </form>
        )}
        {view && (
          <>
            <p className="console-target-path">
              {consoleDisplayPath(view.target.path)}
            </p>
            <div className="console-device-meta">
              <span>
                {busy
                  ? "Consultando…"
                  : view.online
                    ? "Conectado"
                    : "Indisponível"}
              </span>
              <span>{view.target.volume.filesystem}</span>
              <span>
                {!view.online && "Última consulta: "}
                {formatBytes(view.target.volume.free_bytes)} livres de{" "}
                {formatBytes(view.target.volume.total_bytes)}
              </span>
              <span>
                {view.snapshot
                  ? `${games.length} ${games.length === 1 ? "jogo" : "jogos"}`
                  : "Ainda não lido"}
              </span>
            </div>
            <div className="console-toolbar">
              <label className="console-grow">
                Perfil do OPL
                <select
                  aria-label="Perfil do OPL neste destino"
                  value={view.target.profile}
                  onChange={(e) =>
                    void changeProfile(e.target.value as ConsoleProfile)
                  }
                  disabled={busy}
                >
                  {profiles.map(([value, label]) => (
                    <option key={value} value={value}>
                      {label}
                    </option>
                  ))}
                </select>
              </label>
              <button
                type="button"
                className="button-secondary"
                disabled={busy || !view.online}
                onClick={() => void read(selected, true)}
              >
                {busy ? "Lendo jogos…" : "Atualizar biblioteca"}
              </button>
              <button
                type="button"
                className="text-button"
                onClick={() => setConfirmForget(true)}
                disabled={busy}
              >
                Remover cadastro
              </button>
            </div>
            <p className="field-hint">{profileHint(view.target.profile)}</p>
            {confirmForget && (
              <div
                className="console-toolbar"
                role="group"
                aria-label="Confirmar remoção de cadastro"
              >
                <span>Remover só o cadastro? Os arquivos serão mantidos.</span>
                <button
                  type="button"
                  className="button-secondary"
                  onClick={() => void forget()}
                  disabled={busy}
                >
                  Remover cadastro
                </button>
                <button
                  type="button"
                  className="text-button"
                  onClick={() => setConfirmForget(false)}
                >
                  Cancelar
                </button>
              </div>
            )}
          </>
        )}
      </section>
      {error && error !== view?.connection_error && (
        <div className="resource-error console-notice" role="alert">
          {error}
          <button
            type="button"
            className="text-button"
            onClick={() =>
              selected ? void read(selected) : void loadTargets()
            }
            disabled={busy}
          >
            Tentar novamente
          </button>
        </div>
      )}
      {view?.connection_error && (
        <div className="console-notice" role="status">
          {view.connection_error}
        </div>
      )}
      {view?.warnings
        .filter(() => view.target.profile !== "unknown")
        .map((w) => (
          <p className="console-notice" key={w}>
            {w}
          </p>
        ))}
      {busy && (
        <p className="field-hint" role="status">
          Consultando o destino…
        </p>
      )}
      {view?.snapshot && (
        <>
          <div className="console-toolbar console-cache-info">
            <span className="field-hint">
              {view.cached ? "Inventário salvo" : "Leitura concluída"} ·{" "}
              {new Date(view.snapshot.scanned_at).toLocaleString("pt-BR")}
              {!view.online ? " · offline" : ""}
            </span>
            <span className="field-hint">
              Índice e cabeçalhos; sem verificação completa dos jogos.
            </span>
          </div>
          {view.snapshot.issues.length > 0 && (
            <details className="panel console-diagnostics">
              <summary>
                Diagnóstico do destino · {view.snapshot.issues.length}{" "}
                {view.snapshot.issues.length === 1 ? "aviso" : "avisos"}
              </summary>
              <ul>
                {view.snapshot.issues.map((issue, i) => (
                  <li key={i}>{issue}</li>
                ))}
              </ul>
            </details>
          )}
          <div className="catalog-browser-bar console-filters">
            <label className="visually-hidden" htmlFor="console-search">
              Buscar no pendrive
            </label>
            <input
              id="console-search"
              type="search"
              placeholder="Buscar por nome ou serial…"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
            />
            <label>
              Formato
              <select
                aria-label="Formato dos jogos"
                value={method}
                onChange={(e) => setMethod(e.target.value)}
              >
                <option value="all">Todos</option>
                {Array.from(new Set(games.map((g) => g.method)))
                  .sort()
                  .map((m) => (
                    <option key={m} value={m}>
                      {m.toUpperCase()}
                    </option>
                  ))}
              </select>
            </label>
            <label className="console-checkbox">
              <input
                type="checkbox"
                checked={issuesOnly}
                onChange={(e) => setIssuesOnly(e.target.checked)}
              />
              Com avisos
            </label>
            <div role="group" aria-label="Visualização">
              {["grid", "list"].map((value) => (
                <button
                  type="button"
                  className="theme-selection"
                  aria-pressed={layout === value}
                  key={value}
                  onClick={() => setLayout(value)}
                >
                  {value === "grid" ? "Grade" : "Lista"}
                </button>
              ))}
            </div>
            <span className="field-hint">
              {filtered.length} {filtered.length === 1 ? "jogo" : "jogos"} ·{" "}
              {gamesWithWarnings} com avisos
            </span>
          </div>
          {filtered.length === 0 ? (
            <div className="empty-state">
              <h3>
                {games.length
                  ? "Nenhum jogo corresponde aos filtros"
                  : "Nenhum jogo instalado neste destino"}
              </h3>
              <p>Os jogos existentes são lidos de CD, DVD e ul.cfg.</p>
            </div>
          ) : (
            <div
              className={
                layout === "grid" ? "library-grid" : "console-game-list"
              }
            >
              {visible.map((game) => (
                <article
                  key={game.id}
                  className={`library-game-card${layout === "list" ? " console-game-row" : ""}`}
                >
                  <button
                    type="button"
                    className="library-game-cover"
                    onClick={(event) => showDetails(game, event.currentTarget)}
                    aria-label={`Detalhes de ${game.title}`}
                  >
                    <CoverImage
                      title={game.title}
                      src={
                        game.serial
                          ? `https://raw.githubusercontent.com/xlenore/ps2-covers/main/covers/default/${game.serial}.jpg`
                          : null
                      }
                    />
                  </button>
                  <div className="library-game-info">
                    <h3>
                      <button
                        type="button"
                        className="card-open"
                        onClick={(event) =>
                          showDetails(game, event.currentTarget)
                        }
                      >
                        {game.title}
                      </button>
                    </h3>
                    <div className="card-badges-row">
                      <span className="card-badge-chip">
                        {game.method.toUpperCase()}
                      </span>
                      <span className="card-badge-chip">
                        {game.media.toUpperCase()}
                      </span>
                      {game.issues.length > 0 && (
                        <span className="card-badge-chip card-badge-danger">
                          {game.issues.length}{" "}
                          {game.issues.length === 1 ? "aviso" : "avisos"}
                        </span>
                      )}
                    </div>
                    <p className="library-game-meta">
                      {formatBytes(game.bytes)}
                      {game.serial ? ` · ${game.serial}` : ""}
                    </p>
                    {game.method === "ul" && (
                      <p className="field-hint">
                        {game.found_parts} de {game.parts}{" "}
                        {game.parts === 1 ? "parte" : "partes"}
                      </p>
                    )}
                  </div>
                  <div className="library-game-actions">
                    <button
                      type="button"
                      className="button-secondary"
                      onClick={(event) =>
                        showDetails(game, event.currentTarget)
                      }
                    >
                      Ver detalhes
                    </button>
                  </div>
                </article>
              ))}
            </div>
          )}
          {pageCount > 1 && (
            <nav className="console-toolbar" aria-label="Paginação do Console">
              <button
                type="button"
                className="button-secondary"
                disabled={currentPage === 1}
                onClick={() => setPage(currentPage - 1)}
              >
                Anterior
              </button>
              <span>
                Página {currentPage} de {pageCount}
              </span>
              <button
                type="button"
                className="button-secondary"
                disabled={currentPage === pageCount}
                onClick={() => setPage(currentPage + 1)}
              >
                Próxima
              </button>
            </nav>
          )}
          {details && (
            <dialog
              className="console-game-dialog console-diagnostics"
              aria-label={`Detalhes de ${details.title}`}
              ref={(node) => {
                detailsDialog.current = node;
                if (node && !node.open) node.showModal();
              }}
              onCancel={(event) => {
                event.preventDefault();
                closeDetails();
              }}
              onClose={() => setDetails(null)}
            >
              <div className="console-toolbar">
                <h3>{details.title}</h3>
                <button
                  autoFocus
                  type="button"
                  className="text-button"
                  onClick={closeDetails}
                >
                  Fechar detalhes
                </button>
              </div>
              <p className="field-hint">
                {details.method === "ul"
                  ? "Índice e tamanho das partes consultados."
                  : "Identificação por arquivo e inspeção do cabeçalho ISO quando disponível."}{" "}
                Teste no PS2 não realizado.
              </p>
              {details.issues.length > 0 && (
                <ul>
                  {details.issues.map((issue, i) => (
                    <li key={i}>{issue}</li>
                  ))}
                </ul>
              )}
              <details>
                <summary>Arquivos · {details.files.length}</summary>
                <ul>
                  {details.files.map((file) => (
                    <li key={file}>{file}</li>
                  ))}
                </ul>
              </details>
            </dialog>
          )}
        </>
      )}
    </section>
  );
}
