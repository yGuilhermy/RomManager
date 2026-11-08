import { useEffect, useRef, useState } from "react";
import { apiJson } from "../lib/api";
import { loadExtras, peekExtras } from "../lib/extrasCache";
import { formatBytes } from "../lib/format";
import {
  extraLabels,
  localTargets,
  canApplyExtra,
  extraCover,
  type ExtraGame,
  type ExtraItem,
  type ExtraKind,
  type GameResults,
  type ItemResults,
} from "../lib/extras";
import type { InstalledGame } from "../types";
import { useExtraOperation } from "./useExtraOperation";
import { CoverImage } from "./CoverImage";

type Card = {
  name: string;
  kind: string;
  revision: string;
  free_bytes: number | null;
  error: string | null;
};
type Cards = {
  cards: Card[];
  installation_id: string;
  running: boolean;
  tool_ready: boolean;
};
type Destination = {
  item: ExtraItem;
  card: string;
  game: string;
  group: string;
  overwrite: boolean;
  confirmed: boolean;
};
export function ExtrasPage({ installed }: { installed: InstalledGame[] }) {
  const [saveSource, setSaveSource] = useState("gamefaqs");
  const [kind, setKind] = useState<ExtraKind>("saves"),
    [q, setQ] = useState(""),
    [page, setPage] = useState(1);
  const [games, setGames] = useState<GameResults | null>(null),
    [selected, setSelected] = useState<ExtraGame | null>(null),
    [items, setItems] = useState<ItemResults | null>(null);
  const [loading, setLoading] = useState(false),
    [reading, setReading] = useState(false),
    [error, setError] = useState(""),
    [refresh, setRefresh] = useState(0);
  const [destination, setDestination] = useState<Destination | null>(null),
    [cards, setCards] = useState<Cards | null>(null),
    [cardDetail, setCardDetail] = useState<Card | null>(null),
    [targetError, setTargetError] = useState("");
  const [targetReading, setTargetReading] = useState(false);
  const destinationRef = useRef<HTMLElement | null>(null);
  const gamesRefresh = useRef(0), itemsRefresh = useRef(0);
  useEffect(() => {
    if (destination) {
      destinationRef.current?.focus();
      destinationRef.current?.scrollIntoView({ block: "nearest" });
    }
  }, [destination?.item.id]);
  const operation = useExtraOperation();
  useEffect(() => {
    if (selected) return;
    let active = true;
    const path = `/api/extras/games?kind=${kind}&q=${encodeURIComponent(q)}&page=${page}${kind === "saves" ? `&source=${saveSource}` : ""}`;
    const cached=peekExtras<GameResults>(path);
    setGames(cached || null);
    setError("");
    setLoading(!cached);
    const timer = setTimeout(() => {
      const force=refresh>0&&gamesRefresh.current!==refresh;gamesRefresh.current=refresh;
      if(force) setLoading(true);
      const request = () => apiJson<GameResults>(path + (force ? "&refresh=true" : ""));
      void loadExtras(path, request, force)
        .then((data) => {
          if (active) setGames(data);
        })
        .catch((e) => {
          if (active) setError(e instanceof Error ? e.message : String(e));
        })
        .finally(() => {
          if (active) setLoading(false);
        });
    }, cached ? 0 : 250);
    return () => {
      active = false;
      clearTimeout(timer);
    };
  }, [kind, q, page, refresh, saveSource, selected?.key]);
  useEffect(() => {
    if (!selected) {
      setItems(null);
      return;
    }
    let active = true;
    const path = `/api/extras/items?kind=${kind}&key=${encodeURIComponent(selected.key)}`;
    const cached=peekExtras<ItemResults>(path);
    setReading(!cached);
    setItems(cached || null);
    setError("");
    const force=refresh>0&&itemsRefresh.current!==refresh;itemsRefresh.current=refresh;
    if(force)setReading(true);
    const request = () => apiJson<ItemResults>(path + (force ? "&refresh=true" : ""));
    void loadExtras(path, request, force)
      .then((data) => {
        if (active) setItems(data);
      })
      .catch((e) => {
        if (active) setError(e instanceof Error ? e.message : String(e));
      })
      .finally(() => {
        if (active) setReading(false);
      });
    return () => {
      active = false;
    };
  }, [selected?.key, kind, refresh]);
  useEffect(() => {
    if (!destination) return;
    let active = true;
    setCards(null);
    setCardDetail(null);
    setTargetError("");
    setTargetReading(true);
    void apiJson<Cards>("/api/pcsx2/memory-cards?summary=true")
      .then((data) => {
        if (active) setCards(data);
      })
      .catch((e) => {
        if (active) setTargetError(e instanceof Error ? e.message : String(e));
      })
      .finally(() => {
        if (active) setTargetReading(false);
      });
    return () => {
      active = false;
    };
  }, [destination?.item.id]);
  useEffect(() => {
    if (!destination?.card) return;
    let active = true;
    setCardDetail(null);
    setTargetReading(true);
    setTargetError("");
    void apiJson<Cards>(
      `/api/pcsx2/memory-cards?card=${encodeURIComponent(destination.card)}`,
    )
      .then((data) => {
        if (!active) return;
        if (data.installation_id !== cards?.installation_id)
          throw new Error(
            "A instalação mudou. Feche e selecione o destino novamente.",
          );
        setCardDetail(data.cards[0] || null);
      })
      .catch((e) => {
        if (active) setTargetError(e instanceof Error ? e.message : String(e));
      })
      .finally(() => {
        if (active) setTargetReading(false);
      });
    return () => {
      active = false;
    };
  }, [destination?.card, cards?.installation_id]);
  const changeKind = (value: ExtraKind) => {
    setKind(value);
    setSelected(null);
    setItems(null);
    setGames(null);
    setPage(1);
    setDestination(null);
    setError("");
    setRefresh(0);
  };
  const openDestination = (item: ExtraItem) => {
    setDestination({
      item,
      card: "",
      game: localTargets(item, installed)[0]?.id || "",
      group: "",
      overwrite: false,
      confirmed: false,
    });
  };
  const submit = async () => {
    if (!destination || !cards) return;
    const { item } = destination;
    if (
      await operation.start({
        action: "install",
        kind: item.kind,
        game_key: item.game_key,
        item_id: item.id,
        installation_id: cards.installation_id,
        ...(item.kind === "saves"
          ? {
              card: destination.card,
              save_confirmed: destination.confirmed,
              card_revision: cardDetail?.revision,
              overwrite: destination.overwrite,
            }
          : {
              game_id: destination.game,
              crc: item.crc || "",
              group_name: destination.group || undefined,
              overwrite: destination.overwrite,
            }),
      })
    )
      setDestination(null);
  };
  const job = operation.operation?.job,
    result = operation.operation?.result;
  const refreshVisible=()=>{
    if(selected)gamesRefresh.current=refresh+1;else itemsRefresh.current=refresh+1;
    setRefresh((v)=>v+1);
  };
  return (
    <div className="extras-page">
      <header className="page-heading extras-heading">
        <div>
          <h2>Extras</h2>
          <p>Saves, códigos e melhorias para PlayStation 2</p>
        </div>
        <div className="extras-header-actions">
        {kind === "saves" && <select id="extras-save-source" aria-label="Fonte de saves" title="GameFAQs: arquivo de 21/04/2023" value={saveSource} onChange={(e) => {
          setSaveSource(e.target.value); setSelected(null); setPage(1); setDestination(null); setRefresh(0);
        }}>
          <option value="gamefaqs">GameFAQs (2023)</option>
          <option value="apollo">Apollo Saves</option>
        </select>}
        <button
          type="button"
          className="button-secondary"
          disabled={loading || reading}
          onClick={refreshVisible}
        >
          Atualizar fontes
        </button>
        </div>
      </header>
      <div
        className="theme-tabs extras-tabs"
        role="group"
        aria-label="Categorias de extras"
      >
        {(Object.keys(extraLabels) as ExtraKind[]).map((value) => (
          <button
            type="button"
            key={value}
            className={`theme-selection ${kind === value ? "active" : ""}`}
            aria-pressed={kind === value}
            onClick={() => changeKind(value)}
          >
            {extraLabels[value]}
          </button>
        ))}
      </div>
      {operation.error && (
        <p className="a4-notice error" role="alert">
          {operation.error}
        </p>
      )}
      {operation.notice && (
        <p className="a4-notice" role="status">
          {operation.notice}
        </p>
      )}
      {job && (
        <section
          className="panel extras-operation"
          aria-label="Tarefa de extra"
        >
          <div>
            <h3>{result?.title || job.title}</h3>
            <p role="status">{job.error_message || job.current_step}</p>
            {operation.busy && (
              <progress
                value={job.progress}
                max={100}
                aria-label="Progresso do extra"
              />
            )}
            {result?.backup && (
              <p className="field-hint">Backup: {result.backup}</p>
            )}
          </div>
          <div className="extras-actions">
            {operation.busy && (
              <button
                type="button"
                className="button-secondary"
                disabled={job.status === "cancel_requested"}
                onClick={() => void operation.cancel()}
              >
                {job.status === "cancel_requested"
                  ? "Cancelando…"
                  : "Cancelar download"}
              </button>
            )}
            {result && (
              <button
                type="button"
                className="button-secondary"
                disabled={operation.saving}
                onClick={() => void operation.save()}
              >
                {operation.saving ? "Salvando…" : "Salvar arquivo…"}
              </button>
            )}
            {!operation.busy && <button type="button" className="button-quiet" aria-label="Fechar resultado do download" onClick={operation.dismiss}>×</button>}
          </div>
        </section>
      )}
      {error && (
        <div className="a4-notice error" role="alert">
          {error}
          <button
            type="button"
            className="button-quiet"
            onClick={refreshVisible}
          >
            Tentar novamente
          </button>
        </div>
      )}
      {!selected ? (
        <>
          <div className="extras-search">
            <label htmlFor="extras-search">Buscar qualquer jogo</label>
            <input
              id="extras-search"
              type="search"
              placeholder="Nome ou serial"
              value={q}
              onChange={(e) => {
                setQ(e.target.value);
                setPage(1);
                setRefresh(0);
              }}
            />
            <span className="field-hint">
              {loading
                ? "Consultando fonte…"
                : `${games?.total.toLocaleString("pt-BR") || 0} jogos`}
            </span>
          </div>
          <div className="extras-game-grid" aria-busy={loading}>
            {games?.games.map((game) => (
              <button
                type="button"
                className="extras-game theme-selection"
                key={game.key}
                onClick={() => setSelected(game)}
              >
                <div className="extras-game-cover" aria-hidden="true"><CoverImage src={extraCover(game)} title={game.title} /></div>
                <div className="extras-game-copy"><strong>{game.title}</strong>
                <span>
                  {game.serial || (game.key.startsWith("GFQ:") ? "Saves por região e formato" : "Serial não informado")}
                  {game.crcs.length ? ` · ${game.crcs.length} revisões` : ""}
                </span></div>
              </button>
            ))}
          </div>
          {!loading && games && !games.games.length && (
            <p className="resource-empty">
              Nenhum jogo encontrado nesta fonte.
            </p>
          )}
          {!!games && games.total > 24 && (
            <nav className="extras-pagination" aria-label="Páginas de extras">
              <button
                type="button"
                className="button-secondary"
                disabled={page <= 1 || loading}
                onClick={() => setPage((p) => p - 1)}
              >
                Anterior
              </button>
              <span>
                Página {games.page} de {Math.ceil(games.total / 24)}
              </span>
              <button
                type="button"
                className="button-secondary"
                disabled={page >= Math.ceil(games.total / 24) || loading}
                onClick={() => setPage((p) => p + 1)}
              >
                Próxima
              </button>
            </nav>
          )}
        </>
      ) : (
        <>
          <div className="extras-game-heading">
            <button
              type="button"
              className="button-secondary"
              onClick={() => {
                setSelected(null);
                setDestination(null);
              }}
            >
              Voltar aos jogos
            </button>
            <div className="extras-selected-cover" aria-hidden="true"><CoverImage src={extraCover(selected)} title={selected.title}/></div>
            <div>
              <h3>{selected.title}</h3>
              <p className="field-hint">
                {selected.serial || (selected.key.startsWith("GFQ:") ? "Confira a região e o formato de cada save" : "Serial não informado pela fonte")}
              </p>
            </div>
          </div>
          {reading && (
            <p role="status">Consultando {extraLabels[kind].toLowerCase()}…</p>
          )}
          <div className="extras-items-grid">{items?.items.map((item) => (
            <article className="panel extras-item" key={item.id}>
              <div className="extras-item-heading">
                <div>
                  <h3>{item.title}</h3>
                  {item.crc && <p className="field-hint">CRC {item.crc}</p>}
                </div>
                <span>{formatBytes(item.bytes)}</span>
              </div>
              {item.description && (
                <p className="extras-description">{item.description}</p>
              )}
              {!!item.groups.length && (
                <details>
                  <summary>{item.groups.length} códigos ou opções</summary>
                  <ul className="extras-group-list">
                    {item.groups.map((group, index) => (
                      <li key={`${group.name}:${index}`}>
                        <strong>{group.name || "Códigos sem grupo"}</strong>
                        {group.description && <span>{group.description}</span>}
                      </li>
                    ))}
                  </ul>
                </details>
              )}
              <div className="extras-source">
                <a href={item.source_url} target="_blank" rel="noreferrer">
                  {item.source}
                </a>
                <span>{item.license}</span>
              </div>
              <div className="extras-actions">
                <button
                  type="button"
                  className="button-primary"
                  disabled={operation.busy}
                  onClick={() =>
                    void operation.start({
                      action: "download",
                      kind: item.kind,
                      game_key: item.game_key,
                      item_id: item.id,
                    })
                  }
                >
                  Baixar arquivo
                </button>
                <button
                  type="button"
                  className="button-secondary"
                  disabled={
                    operation.busy ||
                    !canApplyExtra(item, installed)
                  }
                  onClick={() => openDestination(item)}
                >
                  Baixar e aplicar…
                </button>
              </div>
              {item.install_note && (
                <p className="field-hint">
                  {!item.installable && "Somente download: "}{item.install_note}
                </p>
              )}
              {kind !== "saves" && !localTargets(item, installed).length && (
                <p className="field-hint">
                  Para aplicar, adicione esta edição à biblioteca. O download
                  avulso está disponível.
                </p>
              )}
            </article>
          ))}</div>
          {!reading && items && !items.items.length && (
            <p className="resource-empty">
              Nenhum arquivo reconhecido para esta edição na fonte.
            </p>
          )}
        </>
      )}
      {(games?.stale || items?.stale) && (
        <p className="a4-notice">
          Sem conexão com a fonte. Usando dados salvos.
        </p>
      )}
      {destination && (
        <section
          className="panel extras-destination"
          ref={destinationRef}
          tabIndex={-1}
          aria-label="Destino do extra"
        >
          <header className="panel-header">
            <h3>Aplicar {extraLabels[destination.item.kind].toLowerCase()}</h3>
            <button
              type="button"
              className="button-quiet"
              onClick={() => setDestination(null)}
            >
              Fechar
            </button>
          </header>
          {targetError && (
            <p className="a4-notice error" role="alert">
              {targetError}
            </p>
          )}
          {cards?.running && (
            <p className="a4-notice">Feche o PCSX2 para aplicar.</p>
          )}
          {destination.item.kind === "saves" ? (
            <>
              {destination.item.game_key.startsWith("GFQ:") && <label className="checkbox-row">
                <input type="checkbox" checked={destination.confirmed} onChange={(e) => setDestination({...destination, confirmed:e.target.checked})}/>
                Conferi a região e a revisão deste save.
              </label>}
              <label className="emulator-form-field">
                Memory card
                <select
                  aria-label="Memory card de destino"
                  value={destination.card}
                  onChange={(e) =>
                    setDestination({ ...destination, card: e.target.value })
                  }
                >
                  <option value="">Selecione um cartão</option>
                  {cards?.cards.map((card) => (
                    <option value={card.name} key={card.name}>
                      {card.name} ·{" "}
                      {card.kind === "folder" ? "Pasta" : "Arquivo"}
                    </option>
                  ))}
                </select>
              </label>
              {cardDetail && (
                <p className="field-hint">
                  {cardDetail.error ||
                    (cardDetail.free_bytes === null
                      ? "Cartão em pasta · espaço do disco conferido antes da importação"
                      : `${formatBytes(cardDetail.free_bytes)} livres`)}
                  . O espaço necessário será conferido após baixar o save.
                </p>
              )}
              {cards && !cards.tool_ready && (
                <p className="a4-notice">
                  Instale o myMCpp em Configurações antes de importar.
                </p>
              )}
            </>
          ) : (
            <>
              <label className="emulator-form-field">
                Jogo instalado
                <select
                  value={destination.game}
                  onChange={(e) =>
                    setDestination({
                      ...destination,
                      game: e.target.value,
                      confirmed: false,
                    })
                  }
                >
                  <option value="">Selecione o jogo</option>
                  {localTargets(destination.item, installed).map((game) => (
                    <option value={game.id} key={game.id}>
                      {game.title}
                    </option>
                  ))}
                </select>
              </label>
              <p className="field-hint">
                Serial {destination.item.serial} · revisão exigida{" "}
                {destination.item.crc}
              </p>
              {destination.item.kind !== "textures" && (
                <label className="emulator-form-field">
                  Código ou patch
                  <select
                    aria-label="Código ou patch a ativar"
                    value={destination.group}
                    onChange={(e) =>
                      setDestination({ ...destination, group: e.target.value })
                    }
                  >
                    <option value="">Escolha uma opção</option>
                    {destination.item.groups
                      .filter((g) => !g.automatic)
                      .map((group) => (
                        <option key={group.name} value={group.name}>
                          {group.name}
                        </option>
                      ))}
                  </select>
                </label>
              )}
              <label className="checkbox-row">
                <input
                  type="checkbox"
                  checked={destination.confirmed}
                  onChange={(e) =>
                    setDestination({
                      ...destination,
                      confirmed: e.target.checked,
                    })
                  }
                />
                Conferi que o CRC deste jogo no PCSX2 é {destination.item.crc}
              </label>
            </>
          )}
          <label className="checkbox-row">
            <input
              type="checkbox"
              checked={destination.overwrite}
              onChange={(e) =>
                setDestination({ ...destination, overwrite: e.target.checked })
              }
            />
            Permitir substituir arquivos ou save existentes, mantendo backup
          </label>
          <div className="extras-actions">
            <button
              type="button"
              className="button-primary"
              disabled={
                operation.busy ||
                targetReading ||
                !cards ||
                cards.running ||
                !!targetError ||
                (kind === "saves"
                  ? !cardDetail || !!cardDetail.error || !cards.tool_ready || (destination.item.game_key.startsWith("GFQ:") && !destination.confirmed)
                  : !destination.game ||
                    !destination.confirmed ||
                    (kind !== "textures" && !destination.group))
              }
              onClick={() => void submit()}
            >
              Baixar e aplicar
            </button>
            <button
              type="button"
              className="button-secondary"
              onClick={() => setDestination(null)}
            >
              Cancelar
            </button>
          </div>
        </section>
      )}
    </div>
  );
}
