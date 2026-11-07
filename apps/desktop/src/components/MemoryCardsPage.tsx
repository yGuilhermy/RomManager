import { useEffect, useState, useRef } from "react";
import { apiJson, cachedPcsx2Json, invalidatePcsx2Cache } from "../lib/api";
import { MemoryCardToolCard } from "./MemoryCardToolCard";
import { BrowsePathButton } from "./BrowsePathButton";
import { saveGeneratedFile } from "../lib/nativeFiles";
type Save = { title: string; serial: string | null; name: string; size: number; files: number };
type Card = {
  name: string;
  kind: string;
  size: number;
  revision: string;
  saves: Save[];
  save_count: number | null;
  free_bytes: number | null;
  error: string | null;
  slots: number[];
};
type State = {
  directory: string;
  cards: Card[];
  running: boolean;
  tool_ready: boolean;
  installation_id: string;
};
type Result = { message: string; path: string; backup: string | null };
export function MemoryCardsPage({ gameSerial, embedded = false }: { gameSerial?: string | null; embedded?: boolean }) {
  const [data, setData] = useState<State | null>(null),
    [selected, setSelected] = useState("");
  const [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [result, setResult] = useState<Result | null>(null);
  const [newName, setNewName] = useState(""),
    [source, setSource] = useState(""),
    [target, setTarget] = useState("");
  const [preview, setPreview] = useState<Result | null>(null),
    [overwrite, setOverwrite] = useState(false),
    [confirmation, setConfirmation] = useState<string | null>(null);
  const [restorePath, setRestorePath] = useState("");
  const [saveSearch, setSaveSearch] = useState(""),
    [shown, setShown] = useState(30);
  const [detail, setDetail] = useState<Card | null>(null);
  const [reading, setReading] = useState(false);
  const operationRef=useRef<HTMLElement|null>(null);
  const [saveOperation,setSaveOperation]=useState<{kind:"delete-save"|"move-save";save:Save}|null>(null);
  useEffect(()=>{if(saveOperation)operationRef.current?.focus();},[saveOperation?.save.name,saveOperation?.kind]);
  const [destination,setDestination]=useState("");
  const [destinationCard,setDestinationCard]=useState<Card|null>(null);
  const [destinationBusy,setDestinationBusy]=useState(false);
  const [replaceDestination,setReplaceDestination]=useState(false);
  useEffect(()=>{
    if (!destination || saveOperation?.kind!=="move-save") {setDestinationCard(null);return;}
    let active=true;setDestinationBusy(true);setDestinationCard(null);setError("");setReplaceDestination(false);
    cachedPcsx2Json<State>(`/api/pcsx2/memory-cards?card=${encodeURIComponent(destination)}`).then(state=>{
      if(!active)return;
      if(state.installation_id!==data?.installation_id) throw new Error("A instalação mudou. Atualize os cartões.");
      const next=state.cards[0];if(!next || next.error) throw new Error(next?.error||"Cartão não encontrado.");
      setDestinationCard(next);
    }).catch(e=>{if(active)setError(e instanceof Error ? e.message : "Não foi possível ler o destino.");}).finally(()=>{if(active)setDestinationBusy(false);});
    return()=>{active=false;};
  },[destination,saveOperation?.kind,data?.installation_id]);
  const [generation, setGeneration] = useState(0);
  const card = detail?.name === selected ? detail : undefined;
  const visibleSaves =
    card?.saves.filter((s) =>
      (s.name + " " + s.title).toLowerCase().includes(saveSearch.toLowerCase()) && (!gameSerial || s.serial === gameSerial),
    ) || [];
  const load = async (force = false) => {
    setError("");
    try {
      if (force) invalidatePcsx2Cache();
      const next = await cachedPcsx2Json<State>("/api/pcsx2/memory-cards?summary=true");
      setData(next);
      if (force) setGeneration((value) => value + 1);
      setSelected((old) =>
        next.cards.some((c) => c.name === old)
          ? old
          : next.cards.find((c) => c.slots.includes(1))?.name ||
            next.cards.find((c) => c.kind === "file")?.name ||
            next.cards[0]?.name ||
            "",
      );
    } catch (e) {
      setError(
        e instanceof Error ? e.message : "Não foi possível ler os cartões.",
      );
    }
  };
  useEffect(() => {
    void load();
  }, []);
  useEffect(() => {
    if (!selected || !data?.installation_id) { setDetail(null); return; }
    let active = true;
    setReading(true);
    setDetail(null);
    cachedPcsx2Json<State>(`/api/pcsx2/memory-cards?card=${encodeURIComponent(selected)}`)
      .then((state) => {
        if (!active) return;
        if (state.installation_id !== data.installation_id) throw new Error("Instalação mudou. Atualize os cartões.");
        setDetail(state.cards[0] || null);
      })
      .catch((e) => { if (active) setError(e instanceof Error ? e.message : "Não foi possível ler o cartão."); })
      .finally(() => { if (active) setReading(false); });
    return () => { active = false; };
  }, [selected, data?.installation_id, generation]);
  const action = async (
    action: string,
    extra: Record<string, unknown> = {},
  ) => {
    if (busy) return;
    setBusy(true);
    setError("");
    setResult(null);
    try {
      const response = await apiJson<Result>("/api/pcsx2/memory-cards/action", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          action,
          card: card?.name || "",
          revision: card?.revision || "",
          installation_id: data?.installation_id || "",
          source,
          ...extra,
        }),
      });
      if (action === "export") {
        const destination = await saveGeneratedFile({ kind: "save", source: response.path });
        if (destination === null) return;
        response.path = destination;
      }
      if (action === "preview") {
        setPreview(response);
        setOverwrite(false);
      } else {
        setResult(response);
        setPreview(null);
        setConfirmation(null);
        setSaveOperation(null);
        setDestination("");
        await load(true);
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };
  const disabled = busy || reading || !data || data.running;
  return (
    <div className="emulator-page emulator-compact memory-page">
      {!embedded && <header className="panel-header emulator-heading">
        <div>
          <h2>Memory cards</h2>
          <p>Cartões e saves da instalação do PCSX2</p>
        </div>
        <button
          className="button-secondary"
          disabled={busy}
          onClick={() => void load(true)}
        >
          Atualizar
        </button>
      </header>}
      {error && (
        <div role="alert" className="a4-notice error">
          {error}
          <button className="button-quiet" onClick={() => void load(true)}>
            Reler dados
          </button>
        </div>
      )}
      {data?.running && (
        <div className="a4-notice" role="status">
          PCSX2 aberto. Feche o emulador para alterar cartões.
        </div>
      )}
      {!data && !error && <p role="status">Lendo cartões…</p>}
      {result && (
        <div className="a4-notice" role="status">
          <strong>{result.message}</strong>
          <code>{result.path}</code>
          {result.backup && <span>Backup: {result.backup}</span>}
        </div>
      )}
      {data && (
        <>
          {!embedded && <MemoryCardToolCard dismissible onChanged={() => void load(true)} />}
          <div className="a4-columns">
            <section className="panel">
              <div className="panel-header">
                <h3>
                  Cartões <span className="a4-count">{data.cards.length}</span>
                </h3>
              </div>
              <div className="memory-card-list">
                {data.cards.map((c) => (
                  <button
                    className={`theme-selection memory-card-choice ${selected === c.name ? "active" : ""}`}
                    aria-pressed={selected === c.name}
                    disabled={busy}
                    key={c.name}
                    onClick={() => {
                      setSaveOperation(null);
                      setDestination("");
                      setSelected(c.name);
                      setSaveSearch("");
                      setShown(30);
                      setPreview(null);
                      setResult(null);
                      setError("");
                    }}
                  >
                    <strong>{c.name}</strong>
                    <span>
                      {c.kind === "folder" ? "Pasta" : "Arquivo"}
                      {c.error ? " · Verificar cartão" : (selected === c.name && card ? ` · ${card.saves.length} saves` : c.save_count === null ? "" : ` · ${c.save_count} saves`)}
                      {c.slots.length ? ` · Slot ${c.slots.join(", ")}` : ""}
                    </span>
                  </button>
                ))}
              </div>
              {!data.cards.length && (
                <p className="field-hint">
                  Nenhum cartão encontrado nesta instalação.
                </p>
              )}
              {!embedded && <><form
                className="a4-inline-form"
                onSubmit={(e) => {
                  e.preventDefault();
                  void action("create", { target: newName });
                }}
              >
                <label htmlFor="new-card">Novo cartão · 8 MB</label>
                <div>
                  <input
                    id="new-card"
                    value={newName}
                    onChange={(e) => setNewName(e.target.value)}
                    placeholder="Nome do cartão"
                  />
                  <button
                    className="button-secondary"
                    disabled={disabled || !data.tool_ready || !newName.trim()}
                  >
                    Criar
                  </button>
                </div>
              </form>
              <details className="a4-path">
                <summary>Pasta dos cartões</summary>
                <code>{data.directory}</code>
              </details></>}
            </section>
            <section className="panel memory-content">
              {card ? (
                <>
                  <div className="panel-header">
                    <div>
                      <h3>{card.name}</h3>
                      <p className="field-hint">
                        {card.free_bytes !== null
                          ? `${(card.free_bytes / 1024).toFixed(0)} KB livres`
                          : "Cartão em pasta"}
                      </p>
                    </div>
                    {!embedded && <button
                      className="button-secondary"
                      disabled={disabled}
                      onClick={() => void action("backup")}
                    >
                      Fazer backup
                    </button>}
                  </div>
                  {!card.error && (
                    <form
                      className="a4-inline-form"
                      onSubmit={(e) => {
                        e.preventDefault();
                        void action("preview");
                      }}
                    >
                      <label htmlFor="save-source">
                        Importar save · PSU ou MAX
                      </label>
                      <div>
                        <input
                          id="save-source"
                          disabled={busy}
                          value={source}
                          onChange={(e) => {
                            setSource(e.target.value);
                            setPreview(null);
                          }}
                          placeholder="Caminho do save no PC"
                        />
                        <BrowsePathButton title="Selecionar save para importar" extensions={["psu", "max"]} current={source} disabled={busy} onSelect={path=>{setSource(path);setPreview(null);}} />
                        <button
                          className="button-secondary"
                          disabled={
                            disabled || !data.tool_ready || !source.trim()
                          }
                        >
                          Analisar save
                        </button>
                      </div>
                    </form>
                  )}
                  {preview && (
                    <div className="a4-preview">
                      <h3>Revisar importação</h3>
                      <p>{preview.message}</p>
                      <p className="field-hint">
                        Confira a região e a revisão do jogo antes de importar.
                      </p>
                      {card.saves.some((s) => s.name === preview.path) && (
                        <label className="checkbox-row">
                          <input
                            type="checkbox"
                            checked={overwrite}
                            onChange={(e) => setOverwrite(e.target.checked)}
                          />
                          Substituir save existente, criando backup
                        </label>
                      )}
                      <div className="a4-actions">
                        <button
                          className="button-secondary"
                          onClick={() => setPreview(null)}
                        >
                          Cancelar
                        </button>
                        <button
                          className="button-primary"
                          disabled={
                            disabled ||
                            (card.saves.some((s) => s.name === preview.path) &&
                              !overwrite)
                          }
                          onClick={() => void action("import", { overwrite })}
                        >
                          Importar save
                        </button>
                      </div>
                    </div>
                  )}
                  {saveOperation && <section className="a4-preview save-operation" ref={operationRef} tabIndex={-1} role="region" aria-label={saveOperation.kind==="move-save" ? "Mover save" : "Excluir save"}>
                    <h3>{saveOperation.kind==="move-save" ? "Mover" : "Excluir"} {saveOperation.save.title || saveOperation.save.name}?</h3>
                    <p className="field-hint">{saveOperation.kind==="move-save" ? "O conteúdo será verificado no destino antes da remoção da origem. Os dois cartões terão backup." : "O save será removido deste cartão. Um backup completo será preservado."}</p>
                    {saveOperation.kind==="move-save" && <>
                      <label className="emulator-form-field">Cartão de destino<select aria-label="Cartão de destino" value={destination} disabled={busy} onChange={e=>setDestination(e.target.value)}><option value="">Selecione outro cartão</option>{data.cards.filter(c=>c.name!==card.name).map(c=><option key={c.name} value={c.name}>{c.name} · {c.kind==="folder" ? "Pasta" : "Arquivo"}</option>)}</select></label>
                      {destinationBusy && <p role="status">Lendo cartão de destino…</p>}
                      {destinationCard && <p className="field-hint">{destinationCard.free_bytes===null ? "Cartão em pasta · limitado pelo espaço do disco" : `${(destinationCard.free_bytes/1024).toFixed(0)} KB livres`}</p>}
                      {destinationCard?.saves.some(s=>s.name===saveOperation.save.name) && <label className="checkbox-row"><input type="checkbox" checked={replaceDestination} disabled={busy} onChange={e=>setReplaceDestination(e.target.checked)} />Substituir o save existente no destino, mantendo backup</label>}
                      {destinationCard?.kind==="file"&&!data.tool_ready && <p className="field-hint">Instale o myMCpp para alterar cartões em arquivo.</p>}
                    </>}
                    <div className="a4-actions"><button type="button" className="button-secondary" disabled={busy} onClick={()=>{setSaveOperation(null);setDestination("");}}>Cancelar</button><button type="button" className="button-primary" disabled={disabled || destinationBusy || (saveOperation.kind==="move-save" && (!destinationCard || (destinationCard.kind==="file"&&!data.tool_ready) || (destinationCard.saves.some(s=>s.name===saveOperation.save.name)&&!replaceDestination)))} onClick={()=>void action(saveOperation.kind,{save:saveOperation.save.name,target:destination,target_revision:destinationCard?.revision||"",overwrite:replaceDestination})}>{busy ? "Aguarde…" : saveOperation.kind==="move-save" ? "Confirmar movimentação" : "Confirmar exclusão"}</button></div>
                  </section>}
                  {card.error ? (
                    <p role="alert" className="a4-notice error">
                      {card.error}
                    </p>
                  ) : (
                    <div className="save-list">
                      <input
                        className="a4-save-search"
                        aria-label="Buscar saves"
                        placeholder="Buscar pelo nome do save"
                        value={saveSearch}
                        onChange={(e) => {
                          setSaveSearch(e.target.value);
                          setShown(30);
                        }}
                      />
                      {visibleSaves.slice(0, shown).map((save) => (
                        <article className="save-row" key={save.name}>
                          <div>
                            <strong>{save.title || "Jogo não identificado"}</strong>
                            <span className="save-directory">{save.name}</span>
                            <span>
                              {(save.size / 1024).toFixed(1)} KB · {save.files}{" "}
                              arquivos
                            </span>
                          </div>
                          <div className="save-row-actions">
                          <button
                            className="button-secondary"
                            disabled={disabled || !data.tool_ready}
                            onClick={() =>
                              void action("export", { save: save.name })
                            }
                          >
                            Exportar PSU
                          </button>
                            <button type="button" className="button-secondary" disabled={disabled || data.cards.length<2 || (card.kind==="file"&&!data.tool_ready)} onClick={()=>{setSaveOperation({kind:"move-save",save});setDestination("");setError("");}}>Mover</button>
                            <button type="button" className="button-quiet danger-text" disabled={disabled || (card.kind==="file"&&!data.tool_ready)} onClick={()=>{setSaveOperation({kind:"delete-save",save});setDestination("");setError("");}}>Excluir</button>
                          </div>
                        </article>
                      ))}
                      {visibleSaves.length > shown && (
                        <button
                          className="button-secondary"
                          onClick={() => setShown((n) => n + 30)}
                        >
                          Mostrar mais saves ({visibleSaves.length - shown})
                        </button>
                      )}
                      {!visibleSaves.length && (
                        <p className="field-hint">
                          Nenhum save encontrado neste cartão para esta busca.
                        </p>
                      )}
                    </div>
                  )}
                  {!embedded && <details className="a4-maintenance">
                    <summary>Administrar cartão</summary>
                    <form
                      className="a4-inline-form"
                      onSubmit={(e) => {
                        e.preventDefault();
                        void action("duplicate", { target });
                      }}
                    >
                      <label htmlFor="duplicate-card">
                        Duplicar com outro nome
                      </label>
                      <div>
                        <input
                          id="duplicate-card"
                          value={target}
                          onChange={(e) => setTarget(e.target.value)}
                          placeholder={
                            card.kind === "file" ? "Copia.ps2" : "Copia"
                          }
                        />
                        <button
                          className="button-secondary"
                          disabled={disabled || !target}
                        >
                          Duplicar
                        </button>
                        <button
                          type="button"
                          className="button-secondary"
                          disabled={
                            disabled ||
                            !target ||
                            !data.tool_ready ||
                            !!card.error
                          }
                          onClick={() => void action("convert", { target })}
                        >
                          {card.kind === "file"
                            ? "Converter em pasta"
                            : "Converter em arquivo"}
                        </button>
                      </div>
                    </form>
                    <form
                      className="a4-inline-form"
                      onSubmit={(e) => {
                        e.preventDefault();
                        setConfirmation("restore");
                      }}
                    >
                      <label htmlFor="restore-card">
                        Restaurar backup do RomManager
                      </label>
                      <div>
                        <input
                          id="restore-card"
                          value={restorePath}
                          onChange={(e) => setRestorePath(e.target.value)}
                          placeholder="Caminho do backup"
                        />
                        <BrowsePathButton title="Selecionar backup em arquivo" extensions={["ps2"]} current={restorePath} disabled={busy} onSelect={setRestorePath} />
                        <BrowsePathButton title="Selecionar backup em pasta" directory current={restorePath} disabled={busy} onSelect={setRestorePath} />
                        <button
                          className="button-secondary"
                          disabled={disabled || !restorePath}
                        >
                          Revisar
                        </button>
                      </div>
                    </form>
                    <button
                      className="button-quiet danger-text"
                      disabled={disabled}
                      onClick={() => setConfirmation("remove")}
                    >
                      Remover cartão
                    </button>
                  </details>}
                  {!embedded && confirmation && (
                    <div className="a4-preview" role="alert">
                      <h3>
                        {confirmation === "remove"
                          ? "Remover este cartão?"
                          : "Restaurar este backup?"}
                      </h3>
                      <p>
                        Uma cópia verificada do cartão atual será preservada.
                      </p>
                      <div className="a4-actions">
                        <button
                          className="button-secondary"
                          onClick={() => setConfirmation(null)}
                        >
                          Cancelar
                        </button>
                        <button
                          className="button-primary"
                          disabled={disabled}
                          onClick={() =>
                            void action(
                              confirmation,
                              confirmation === "restore"
                                ? { source: restorePath }
                                : {},
                            )
                          }
                        >
                          Confirmar
                        </button>
                      </div>
                    </div>
                  )}
                </>
              ) : (
                <p className="field-hint">
                  {reading ? "Lendo cartão…" : "Selecione um cartão para ver os saves."}
                </p>
              )}
            </section>
          </div>
        </>
      )}
    </div>
  );
}
