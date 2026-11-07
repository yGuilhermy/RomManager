import { useState } from "react";
import { apiJson } from "../lib/api";
import { formatBytes } from "../lib/format";
import type { InstalledGame } from "../types";
import type { ResourceKind, ResourcePlan } from "../lib/gameResourceTypes";
import { useGameResources } from "./useGameResources";
import { MemoryCardsPage } from "./MemoryCardsPage";
import { CoverImage } from "./CoverImage";
import { BrowsePathButton } from "./BrowsePathButton";
export type { ResourceKind } from "../lib/gameResourceTypes";

const labels={covers:"Capas",saves:"Saves",cheats:"Cheats",patches:"Patches",textures:"Texturas"};
const descriptions={covers:"Capa da biblioteca e do emulador",saves:"Saves deste jogo nos seus memory cards",cheats:"Controle a execução e escolha os códigos do jogo",patches:"Correções do emulador e modificações opcionais",textures:"Gerencie a substituição de texturas deste jogo"};
const controls={
  cheats:{title:"Execução de cheats",description:"Aplica os códigos selecionados abaixo. Desativar mantém suas seleções.",on:"Ativar execução de cheats",off:"Desativar execução de cheats"},
  patches:{title:"Correções automáticas de compatibilidade",description:"Correções da base do PCSX2. Os patches opcionais abaixo têm controles próprios.",on:"Ativar correções automáticas",off:"Desativar correções automáticas"},
  textures:{title:"Substituição de texturas",description:"Usa os arquivos de texturas já instalados para este jogo.",on:"Ativar texturas do jogo",off:"Desativar texturas do jogo"},
};

export function GameResourcesPanel({game,kind}:{game:InstalledGame;kind:ResourceKind}) {
  const data=useGameResources(game.id,game.serial,kind);
  const [source,setSource]=useState("");
  const [confirmed,setConfirmed]=useState(false);
  const [plan,setPlan]=useState<ResourcePlan|null>(null);
  const [importOpen,setImportOpen]=useState(false);
  const [overwriteCover,setOverwriteCover]=useState(false);
  const isNative=kind==="cheats"||kind==="patches"||kind==="textures";
  const native=data.native;
  const validCrc=/^[0-9A-F]{8}$/i.test(data.crc);
  const anyPending=Object.keys(data.pending).length>0;
  const canChange=!!data.context&&!!native&&validCrc&&!data.context.running;
  const pendingLabel=(key:string,fallback:string)=>data.pending[key]==="queued"?"Na fila…":data.pending[key]==="saving"?"Salvando…":fallback;
  const control=isNative?controls[kind]:null;
  const enabled=isNative?native?.[kind]:false;
  const groups=native?.groups.filter(group=>group.kind===kind)||[];
  const files=native?.files.filter(file=>file.kind===kind)||[];
  const managed=data.context?.plans.filter(p=>p.game_id===game.id&&p.kind===kind&&p.applied&&!p.undone)||[];
  const renderError=(key:string)=>data.errors[key]&&<p className="resource-error" role="alert">{data.errors[key]}</p>;
  const imported=async(action:string,id?:string)=>{
    const result=await data.install(action,source,id||plan?.id);
    if(result)setPlan(action==="preview"?result:null);
  };
  return <div className="game-resource-pane">
    <header className="resource-heading">
      <div><h3>{labels[kind]}</h3><p>{descriptions[kind]}</p></div>
      {isNative&&<button className="button-secondary" disabled={data.reading} onClick={()=>void data.refresh()}>{data.reading?"Lendo recursos…":`Recarregar ${labels[kind].toLowerCase()}`}</button>}
    </header>
    {renderError("read")}
    {data.context?.running&&<p className="a4-notice" role="status">Feche o PCSX2 para alterar os recursos deste jogo</p>}
    <p className="resource-feedback" role="status" aria-live="polite">{data.notice}</p>
    {isNative&&<>
      <div className="resource-revision">
        <label htmlFor="game-resource-crc">Revisão (CRC)</label>
        <input id="game-resource-crc" aria-label="CRC para opções do jogo" value={data.crc} list="game-resource-crcs" maxLength={8} disabled={anyPending||data.reading} placeholder="8 dígitos" onChange={e=>{data.setCrc(e.target.value.toUpperCase());setPlan(null);setConfirmed(false);}} />
        <datalist id="game-resource-crcs">{data.revisionOptions.map(value=><option key={value} value={value}/>)}</datalist>
        {data.crc!==native?.crc&&<button className="button-quiet" disabled={!validCrc||data.reading} onClick={()=>void data.refresh()}>Ler esta revisão</button>}
        <span>{game.serial||"Serial não identificado"}</span>
      </div>
      {!native&&!data.reading&&<p className="field-hint">Escolha a revisão para carregar os recursos disponíveis</p>}
      {control&&<section className="resource-control" aria-label={control.title}>
        <div><h4>{control.title}</h4><p>{control.description}</p></div>
        <div className="resource-control-action">
          <span className={`resource-state ${enabled?"is-on":""}`}>{native?(enabled?"Ativada":"Desativada"):"Não consultada"}</span>
          <button className="button-secondary" disabled={!canChange||!!data.pending.master} onClick={()=>void data.toggle(!enabled)}>{pendingLabel("master",enabled?control.off:control.on)}</button>
        </div>
        {renderError("master")}
      </section>}
      {native?.warnings.map(warning=><p className="a4-notice" key={warning} role="status">{warning}</p>)}
      {(kind==="cheats"||kind==="patches")&&<section className="resource-list-section" aria-label={kind==="patches"?"Patches disponíveis":"Cheats disponíveis"}>
        <div className="resource-section-heading"><h4>{kind==="patches"?"Patches opcionais":"Códigos disponíveis"}</h4><span>{groups.length} {groups.length===1?"item":"itens"}</span></div>
        {groups.map(group=>{
          const key=`group:${group.name}`,selected=group.selected??group.enabled;
          const label=selected?(group.enabled?"Desativar":"Desmarcar"):"Ativar";
          return <article className="resource-option" key={group.name}>
            <div className="resource-option-copy"><h5>{group.name||"Códigos sem grupo"}</h5><p className="resource-origin">{group.bundled?"Incluído no PCSX2":"Arquivo local"}{group.author?` · ${group.author}`:""}</p>{group.description&&<p className="resource-description">{group.description}</p>}{renderError(key)}</div>
            <div className="resource-option-action">
              <span className={`resource-state ${group.enabled?"is-on":""}`}>{group.enabled?"Ativo":selected?"Selecionado · execução desativada":"Inativo"}</span>
              {group.automatic?<span className="field-hint">Controle pelo arquivo ou pela execução geral</span>:<button className="button-secondary" aria-label={`${label} ${group.name}`} aria-pressed={selected} disabled={!canChange||!!data.pending[key]} onClick={()=>void data.toggle(!selected,group.name)}>{pendingLabel(key,label)}</button>}
            </div>
          </article>;
        })}
        {!groups.length&&<p className="resource-empty">{data.reading?"Consultando os arquivos do PCSX2…":`Nenhum ${kind==="patches"?"patch opcional":"código"} encontrado para esta revisão`}</p>}
      </section>}
      {kind==="textures"&&<section className="resource-list-section">
        <div className="resource-section-heading"><h4>Texturas instaladas</h4><span>{files.length} arquivos identificados</span></div>
        <p className="field-hint">{files.length?"O pacote está disponível para a substituição de texturas deste jogo":"Importe um pacote abaixo para adicionar texturas ao jogo"}</p>
      </section>}
      {!!files.length&&<details className="resource-files"><summary>Ver {files.length} arquivos locais</summary><ul>{files.slice(0,30).map(file=><li key={file.path} title={file.path}>{file.name}</li>)}</ul>{files.length>30&&<p className="field-hint">Mais {files.length-30} arquivos na pasta</p>}</details>}
    </>}
    {kind==="covers"&&<section className="resource-cover-editor">
      <div className="resource-cover-preview"><CoverImage src={game.cover_url} title={game.title} region={game.region||""}/></div>
      <div><h4>Capa no PCSX2</h4><p className="field-hint">Copie a capa atual da biblioteca para este jogo no emulador</p>
        <label className="checkbox-row"><input type="checkbox" checked={overwriteCover} disabled={!!data.pending.cover} onChange={e=>setOverwriteCover(e.target.checked)}/>Substituir a capa existente no PCSX2</label>
        <button className="button-secondary" disabled={!!data.pending.cover} onClick={()=>void data.run("cover",async()=>{const result=await apiJson<{exported:number;preserved:number}>("/api/pcsx2/covers/export",{method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify({game_id:game.id,overwrite:overwriteCover})});data.report(result.exported?"Capa copiada para o PCSX2":result.preserved?"A capa existente foi preservada":"Nenhuma capa local disponível para copiar");})}>{pendingLabel("cover","Copiar capa para o PCSX2")}</button>
        {renderError("cover")}
      </div>
    </section>}
    {kind==="saves"&&<MemoryCardsPage embedded gameSerial={game.serial}/>}
    {isNative&&<details className="resource-import" open={importOpen||!!plan} onToggle={e=>{setImportOpen(e.currentTarget.open);if(!e.currentTarget.open)setPlan(null);}}>
      <summary>{kind==="textures"?"Importar pacote de texturas":`Importar ${kind==="cheats"?"cheats":"patches"} de um arquivo PNACH`}</summary>
      <form onSubmit={e=>{e.preventDefault();void imported("preview");}}>
        <div className="resource-source-row"><label className="emulator-form-field" htmlFor="resource-source">{kind==="textures"?"Pacote ZIP com replacements/ (PNG ou DDS)":"Arquivo PNACH no computador"}<input id="resource-source" value={source} disabled={!!data.pending.import} onChange={e=>{setSource(e.target.value);setPlan(null);}} placeholder={kind==="textures"?"C:\\Packs\\texturas.zip":"C:\\Cheats\\SERIAL_CRC.pnach"}/></label>
        <BrowsePathButton title={kind==="textures"?"Selecionar pacote de texturas":"Selecionar arquivo PNACH"} extensions={kind==="textures"?["zip"]:["pnach"]} current={source} disabled={!!data.pending.import} onSelect={path=>{setSource(path);setPlan(null);}} />
        </div>
        {kind!=="textures"&&<label className="checkbox-row"><input type="checkbox" checked={confirmed} disabled={!!data.pending.import} onChange={e=>setConfirmed(e.target.checked)}/>Conferi o CRC e a revisão do jogo</label>}
        <div className="resource-form-actions"><button className="button-secondary" disabled={!data.context||data.context.running||!!data.pending.import||!source.trim()||!game.serial||(kind!=="textures"&&(!confirmed||!validCrc))}>{pendingLabel("import","Revisar arquivo antes de instalar")}</button></div>
      </form>
      {renderError("import")}
      {plan&&<section className="a4-preview"><h4>Revisar instalação</h4><p>{plan.files.length} arquivos · {formatBytes(plan.total_bytes)} · {plan.files.filter(f=>f.previous).length} substituições com backup</p><details><summary>Destino e arquivos</summary><code className="resource-destination">{plan.destination}</code><ul>{plan.files.slice(0,30).map(file=><li key={file.relative}>{file.relative}</li>)}</ul></details><div className="resource-form-actions"><button type="button" className="button-quiet" disabled={!!data.pending.import} onClick={()=>setPlan(null)}>Descartar prévia</button><button type="button" className="button-primary" disabled={!!data.pending.import||data.context?.running} onClick={()=>void imported("apply")}>{pendingLabel("import",`Instalar ${kind==="textures"?"texturas":labels[kind].toLowerCase()}`)}</button></div></section>}
    </details>}
    {!!managed.length&&<section className="resource-managed"><div className="resource-section-heading"><h4>Instalações pelo RomManager</h4><span>{managed.length}</span></div>{managed.map(item=><article className="resource-option" key={item.id}><div className="resource-option-copy"><h5>{item.source.split(/[\\/]/).pop()}</h5><p className="field-hint">{item.files.length} arquivos · {formatBytes(item.total_bytes)}</p>{renderError(`undo:${item.id}`)}</div><button className="button-secondary" aria-label={`Remover instalação ${item.source.split(/[\\/]/).pop()}`} disabled={!!data.pending[`undo:${item.id}`]||data.context?.running} onClick={()=>void imported("undo",item.id)}>{pendingLabel(`undo:${item.id}`,"Remover e restaurar anteriores")}</button></article>)}</section>}
  </div>;
}
