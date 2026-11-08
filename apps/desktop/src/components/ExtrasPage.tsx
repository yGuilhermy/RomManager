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
import { SaveExtrasList } from "./SaveExtrasList";

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
  const [browseSort,setBrowseSort]=useState("popular"), [letter,setLetter]=useState(""), [perPage,setPerPage]=useState(24);
  const [jumpPage,setJumpPage]=useState("");
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
    const path = `/api/extras/games?kind=${kind}&q=${encodeURIComponent(q)}&page=${page}${kind === "saves" ? `&source=${saveSource}` : ""}&sort=${browseSort}&letter=${encodeURIComponent(letter)}&per_page=${perPage}`;
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
  }, [kind, q, page, refresh, saveSource, selected?.key,browseSort,letter,perPage]);
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
  const renderItemActions=(item:ExtraItem)=><div className="extras-actions">
    <button type="button" className="button-primary" disabled={operation.busy} onClick={()=>void operation.start({action:"download",kind:item.kind,game_key:item.game_key,item_id:item.id})}>Baixar arquivo</button>
    <button type="button" className="button-secondary" disabled={operation.busy || !canApplyExtra(item, installed)} onClick={()=>openDestination(item)}>Baixar e aplicar…</button>
  </div>;
  const totalPages=games?.total_pages ?? Math.max(1,Math.ceil((games?.total || 0)/perPage));
  const currentPage=games?.page ?? page;
  return (
    <div className="extras-page">
      <header className="catalog-heading panel extras-heading">
        <div>
          <p className="field-hint">PLAYSTATION 2</p>
          <h1>Extras</h1>
        </div>
        <div className="extras-heading-summary">
        {!selected && <span className="field-hint">{(games?.total || 0).toLocaleString("pt-BR")} resultados</span>}
        <div className="extras-header-actions">
        {kind === "saves" && <select className="compact-input" id="extras-save-source" aria-label="Fonte de saves" title="GameFAQs: arquivo de 21/04/2023" value={saveSource} onChange={(e) => {
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
        </div>
      </header>
      <div className="catalog-toolbar extras-catalog-toolbar">
      <div
        className="sort-tabs"
        role="group"
        aria-label="Categorias de extras"
      >
        {(Object.keys(extraLabels) as ExtraKind[]).map((value) => (
          <button
            type="button"
            key={value}
            className={`sort-tab-btn${kind === value ? " active" : ""}`}
            aria-pressed={kind === value}
            onClick={() => changeKind(value)}
          >
            {extraLabels[value]}
          </button>
        ))}
      </div>
      {!selected && <label className="inline-field" htmlFor="extras-sort">Ordem<select id="extras-sort" aria-label="Ordem" value={browseSort} onChange={event=>{setBrowseSort(event.target.value);setPage(1);setRefresh(0);}}>
        <option value="popular">Populares</option><option value="title_asc">A–Z</option><option value="title_desc">Z–A</option>
      </select></label>}
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
          <div className="alphabet-bar">
            <nav className="alphabet-nav" aria-label="Índice alfabético de extras">{["","#",..."ABCDEFGHIJKLMNOPQRSTUVWXYZ"].map(value=><button type="button" key={value || "all"} className={`alphabet-btn${letter===value?" active":""}`} aria-pressed={letter===value} onClick={()=>{setLetter(value);setPage(1);setRefresh(0);if(value)setBrowseSort("title_asc");}}>{value || "Todos"}</button>)}</nav>
            <div className="catalog-tools">
            <label className="visually-hidden" htmlFor="extras-search">Buscar qualquer jogo</label>
            <input
              id="extras-search"
              className="compact-input"
              type="search"
              placeholder="Buscar nos extras…"
              value={q}
              onChange={(e) => {
                setQ(e.target.value);
                setPage(1);
                setRefresh(0);
              }}
            />
            <label className="catalog-page-size" htmlFor="extras-page-size">Por página<select id="extras-page-size" aria-label="Por página" value={perPage} onChange={event=>{setPerPage(Number(event.target.value));setPage(1);setRefresh(0);}}>{[12,24,48,96].map(value=><option key={value}>{value}</option>)}</select></label>
            </div>
          </div>
          {loading && !games && <div className="loader-state" role="status"><div className="spinner"/><p>Carregando extras…</p></div>}
          <div className="mods-grid extras-game-grid" aria-busy={loading}>
            {games?.games.map((game) => (
              <article
                className="mod-card extras-game"
                key={game.key}
              >
                <div className="mod-thumb-wrap"><button type="button" className="catalog-cover-button mod-thumb extras-game-cover" aria-label={`Detalhes de ${game.title}`} onClick={()=>setSelected(game)}><CoverImage src={extraCover(game)} title={game.title}/></button></div>
                <div className="mod-meta">
                  <h3 className="mod-title"><button type="button" className="card-open" onClick={()=>setSelected(game)}>{game.title}</button></h3>
                  <div className="card-badges-row"><span className="card-badge-chip card-badge-format">{extraLabels[kind]}</span>{kind==="saves" && <span className="card-badge-chip">{saveSource==="gamefaqs"?"GameFAQs":"Apollo"}</span>}</div>
                  {(game.serial || game.crcs.length>0) && <p className="mod-author">{[game.serial,game.crcs.length?`${game.crcs.length} revisões`:null].filter(Boolean).join(" · ")}</p>}
                  <div className="card-action-bar"><button type="button" className="button-secondary" onClick={()=>setSelected(game)}>Ver {extraLabels[kind].toLowerCase()}</button></div>
                </div>
              </article>
            ))}
          </div>
          {!loading && games && !games.games.length && (
            <div className="empty-state">
              <h3>Nenhum título encontrado</h3><p>Nenhum jogo corresponde aos filtros ou à pesquisa.</p>
              {(q || letter) && <button type="button" className="button-secondary" onClick={()=>{setQ("");setLetter("");setPage(1);setRefresh(0);}}>Limpar filtros</button>}
            </div>
          )}
          {!!games && games.total > 0 && (
            <nav className="pagination" aria-label="Páginas de extras">
              <button type="button" className="page-btn" disabled={currentPage<=1 || loading} onClick={()=>setPage(1)}>Primeira</button>
              <button type="button" className="page-btn" disabled={currentPage<=1 || loading} onClick={()=>setPage(current=>Math.max(1,current-1))}>Anterior</button>
              <span className="pagination-current">{currentPage} de {totalPages}</span>
              <button type="button" className="page-btn" disabled={currentPage>=totalPages || loading} onClick={()=>setPage(current=>Math.min(totalPages,current+1))}>Próxima</button>
              <button type="button" className="page-btn" disabled={currentPage>=totalPages || loading} onClick={()=>setPage(totalPages)}>Última</button>
              <form className="page-jump" onSubmit={event=>{event.preventDefault();const target=Number(jumpPage);if(Number.isInteger(target)&&target>=1&&target<=totalPages){setPage(target);setJumpPage("");}}}>
                <label htmlFor="extras-jump-page">Ir para</label><input id="extras-jump-page" type="number" min="1" max={totalPages} value={jumpPage} onChange={event=>setJumpPage(event.target.value)}/><button type="submit" className="page-btn" disabled={loading}>Ir</button>
              </form>
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
          {kind === "saves" ? <SaveExtrasList items={items?.items || []} actions={renderItemActions} /> : <div className="extras-items-grid">{items?.items.map((item) => (
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
              {renderItemActions(item)}
              {item.install_note && (
                <p className="field-hint">
                  {!item.installable && "Somente download: "}{item.install_note}
                </p>
              )}
              {!localTargets(item, installed).length && (
                <p className="field-hint">
                  Para aplicar, adicione esta edição à biblioteca. O download
                  avulso está disponível.
                </p>
              )}
            </article>
          ))}</div>}
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
