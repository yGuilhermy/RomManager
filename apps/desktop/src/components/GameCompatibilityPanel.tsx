export type CompatibilityEntry={
  title:string;serial:string;crc?:string|null;region?:string|null;status:string;
  last_tested_version?:string|null;last_tested_date?:string|null;wiki_link?:string|null;forum_link?:string|null;
};
function statusLabel(status:string) {
  const value=status.toLowerCase();
  if(value.includes("unplayable"))return "Não jogável";
  if(value.includes("perfect"))return "Perfeito";
  if(value.includes("playable"))return "Jogável";
  if(value.includes("ingame"))return "Entra no jogo";
  if(value.includes("menu"))return "Menus";
  return status;
}
export function GameCompatibilityPanel({entry,loading,title,region,onRefresh}:{entry:CompatibilityEntry|null|undefined;loading:boolean;title:string;region:string;onRefresh:()=>void}) {
  const good=!!entry&&/^(perfect|playable)/i.test(entry.status);
  return <section className="game-resource-pane compatibility-pane">
    <header className="resource-heading"><div><h3>Compatibilidade com o PCSX2</h3><p>Registro da base de compatibilidade do emulador</p></div><button className="button-secondary" disabled={loading} onClick={onRefresh}>{loading?"Consultando…":"Reconsultar compatibilidade"}</button></header>
    {loading&&!entry?<p className="resource-empty" role="status">Carregando o relatório deste jogo…</p>:entry?<>
      <div className="compatibility-result"><div><span className="resource-origin">Estado reportado</span><strong className={good?"compatibility-good":""}>{statusLabel(entry.status)}</strong></div><div><span className="resource-origin">Versão testada</span><strong>{entry.last_tested_version?`PCSX2 ${entry.last_tested_version.replace(/^PCSX2\s*/i,"").replace(/^v/i,"")}`:"Não informada"}</strong></div></div>
      <dl className="resource-facts"><div><dt>Serial</dt><dd>{entry.serial||"Não informado"}</dd></div><div><dt>Revisão (CRC)</dt><dd>{entry.crc||"Não informada"}</dd></div><div><dt>Região</dt><dd>{entry.region?.toUpperCase()||region}</dd></div>{entry.last_tested_date&&<div><dt>Último teste registrado</dt><dd>{entry.last_tested_date.split("T")[0]}</dd></div>}</dl>
      <p className="field-hint">Este registro não substitui o teste da sua revisão e da sua instalação</p>
    </>:<div className="resource-empty"><h4>Relatório não disponível nesta consulta</h4><p>Consulte a wiki para procurar informações desta edição do jogo</p></div>}
    <div className="resource-link-actions"><a className="button-secondary" href={entry?.wiki_link||`https://wiki.pcsx2.net/index.php?search=${encodeURIComponent(title)}`} target="_blank" rel="noreferrer">{entry?.wiki_link?"Abrir relatório na wiki":"Pesquisar jogo na wiki"}</a>{entry?.forum_link&&<a className="button-secondary" href={entry.forum_link.replace(/&amp;/g,"&")} target="_blank" rel="noreferrer">Abrir discussão no fórum</a>}</div>
  </section>;
}
