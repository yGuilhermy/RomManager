import { MemoryCardToolCard } from "./MemoryCardToolCard";
import { BrowsePathButton } from "./BrowsePathButton";
import { apiFetch } from "../lib/api";
import { useState } from "react";
import type { ValidationResult,CoverJob } from "../types";
const credits = [
    { title: "Projeto", links: [
        { name: "yGuilhermy e contribuidores", url: "https://github.com/yGuilhermy" },
    ], detail: "Desenvolvimento do RomManager e projeto de origem." },
    { title: "Emulador e cartões", links: [
        { name: "PCSX2", url: "https://pcsx2.net/" },
        { name: "myMCpp", url: "https://github.com/PCSX2/myMCpp" },
        { name: "PCSX2 Patches", url: "https://github.com/PCSX2/pcsx2_patches" },
    ], detail: "Emulador, ferramenta de memory cards e patches." },
    { title: "Saves e comunidade", links: [
        { name: "GameFAQs e autores dos saves", url: "https://gamefaqs.gamespot.com/ps2/saves" },
        { name: "Arquivo GameFAQs de 2023", url: "https://archive.org/details/gamefaqs_savegames" },
        { name: "Apollo Saves · bucanero", url: "https://github.com/bucanero/apollo-saves" },
        { name: "RetroAchievements", url: "https://retroachievements.org/" },
    ], detail: "Saves e consulta de conquistas." },
    { title: "Cheats e texturas", links: [
        { name: "PCSX2 Cheats Collection · xs1l3n7x", url: "https://github.com/xs1l3n7x/pcsx2_cheats_collection" },
        { name: "rac · Made-in-Slovakia", url: "https://github.com/Made-in-Slovakia/rac" },
    ], detail: "Códigos e texturas compartilhados pela comunidade." },
    { title: "Capas e catálogo", links: [
        { name: "ps2-covers · xlenore", url: "https://github.com/xlenore/ps2-covers" },
        { name: "Libretro Thumbnails", url: "https://github.com/libretro-thumbnails/Sony_-_PlayStation_2" },
        { name: "Internet Archive", url: "https://archive.org/" },
        { name: "CDRomance", url: "https://cdromance.org/" },
        { name: "Wikipédia", url: "https://www.wikipedia.org/" },
        { name: "PCSX2 Wiki", url: "https://wiki.pcsx2.net/" },
    ], detail: "Capas, catálogo, traduções e informações dos jogos." },
    { title: "Tecnologias", links: [
        { name: "Tauri", url: "https://tauri.app/" },
        { name: "React", url: "https://react.dev/" },
        { name: "Vite", url: "https://vite.dev/" },
        { name: "Rust", url: "https://www.rust-lang.org/" },
        { name: "Axum", url: "https://github.com/tokio-rs/axum" },
        { name: "Tokio", url: "https://tokio.rs/" },
        { name: "Reqwest", url: "https://github.com/seanmonstar/reqwest" },
        { name: "SQLite", url: "https://sqlite.org/" },
    ], detail: "E todas as pessoas que mantêm as bibliotecas usadas pelo projeto." },
];
interface Props {
    configs:Record<string,string>; onChange:(key:string,value:string)=>void; busy:boolean;
    validations:[ValidationResult|null,ValidationResult|null,ValidationResult|null];
    onSaveRoms:()=>Promise<void>; onSaveEmulator:()=>Promise<void>; onSaveCovers:()=>Promise<void>;
    onSaveAutoCovers:(value:boolean)=>Promise<void>; onSaveMaxConcurrent?:(value:string)=>Promise<void>; onOpenFolder:(path:string)=>Promise<void>;
    onRegenerate:()=>Promise<void>; covers:CoverJob|null; logs:string[]; onRefreshLogs:()=>void; onClearLogs:()=>Promise<void>; version:string; onOpenWizard?:()=>void;
}
export function SettingsPage(props:Props) {
    const [savingPreference,setSavingPreference]=useState(false);
    const [nativeValidation,setNativeValidation]=useState<ValidationResult|null>(null);
    const saveDataDirectory=async()=>{
        try {
            await apiFetch("/api/config",{method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify({key:"pcsx2_data_dir",value:(props.configs.pcsx2_data_dir||"").trim().replace(/^["']|["']$/g,"")})});
            setNativeValidation({valid:true,message:"Pasta de dados atualizada."});
        } catch(e) {setNativeValidation({valid:false,message:e instanceof Error ? e.message : "Não foi possível alterar a pasta."});}
    };
    const fields=[
        {key:"roms_directory",label:"Pasta de ROMs",hint:"Diretório usado para sincronizar a biblioteca e instalar downloads.",placeholder:"D:\\Jogos\\PS2",save:props.onSaveRoms,validation:props.validations[0]},
        {key:"pcsx2_executable_path",label:"Executável do PCSX2",hint:"Selecione o caminho completo de pcsx2-qt.exe ou pcsx2.exe.",placeholder:"C:\\Program Files\\PCSX2\\pcsx2-qt.exe",save:props.onSaveEmulator,validation:props.validations[1]},
        {key:"pcsx2_data_dir",label:"Pasta de dados do PCSX2",hint:"Opcional. Use a pasta existente com inis, bios e memorycards. Em branco usa a localização nativa.",placeholder:"Detectar pela instalação",save:saveDataDirectory,validation:nativeValidation},
        {key:"covers_directory",label:"Pasta de capas",hint:"Imagens pessoais são preservadas durante a regeneração.",placeholder:"D:\\Jogos\\PS2\\covers",save:props.onSaveCovers,validation:props.validations[2]},
    ];
    return <div className="settings-page-v2">
        <header className="page-heading settings-page-header"><div><h2>Configurações</h2><p>Biblioteca, emulador e capas</p></div>{props.onOpenWizard && <button type="button" className="button-secondary" onClick={props.onOpenWizard}>Refazer configurações iniciais</button>}</header>
        <section className="panel settings-paths"><h3>Biblioteca e emulador</h3>
            {fields.map(field=><div className="settings-field" key={field.key}>
                <label htmlFor={`settings-${field.key}`}>{field.label}</label><p id={`hint-${field.key}`} className="field-hint">{field.hint}</p>
                <div className="settings-path-input"><input id={`settings-${field.key}`} aria-describedby={`hint-${field.key}`} value={props.configs[field.key] || ""} placeholder={field.placeholder}
                    disabled={props.busy || field.key === "covers_directory" && !!props.covers?.running} onChange={event=>props.onChange(field.key,event.target.value)} />
                    <BrowsePathButton title={`Selecionar ${field.label.toLowerCase()}`} directory={field.key!=="pcsx2_executable_path"} extensions={field.key==="pcsx2_executable_path"?["exe"]:undefined} current={props.configs[field.key]} disabled={props.busy || field.key === "covers_directory" && !!props.covers?.running} onSelect={path=>props.onChange(field.key,path)} />
                    <button type="button" className="button-secondary" disabled={props.busy} onClick={()=>void field.save()}>Salvar e validar</button>
                    {field.key!=="pcsx2_executable_path" && <button type="button" className="button-quiet" disabled={!props.configs[field.key]} onClick={()=>void props.onOpenFolder(props.configs[field.key])}>Abrir pasta</button>}
                </div>
                {field.validation && <p className={`field-validation ${field.validation.valid?"valid":"invalid"}`} role="status">{field.validation.valid?"✓":"!"} {field.validation.message}</p>}
            </div>)}
        </section>
        <MemoryCardToolCard />
        <section className="panel"><h3>Fila e downloads em lote</h3>
            <div className="settings-field">
                <label htmlFor="settings-max-concurrent">Downloads ativos simultâneos</label>
                <p className="field-hint">Define quantos arquivos são baixados e extraídos ao mesmo tempo na fila em massa. Os demais aguardam na fila.</p>
                <div className="settings-path-input">
                    <select id="settings-max-concurrent" value={props.configs.max_concurrent_downloads || "2"} disabled={savingPreference || props.busy} onChange={async event=>{
                        const val = event.target.value;
                        props.onChange("max_concurrent_downloads", val);
                        if (props.onSaveMaxConcurrent) {
                            setSavingPreference(true);
                            try { await props.onSaveMaxConcurrent(val); } finally { setSavingPreference(false); }
                        }
                    }}>
                        <option value="1">1 por vez (menor uso de rede e disco)</option>
                        <option value="2">2 simultâneos (padrão recomendado)</option>
                        <option value="3">3 simultâneos</option>
                        <option value="4">4 simultâneos</option>
                        <option value="5">5 simultâneos (conexões rápidas)</option>
                    </select>
                </div>
            </div>
        </section>
        <section className="panel"><h3>Capas automáticas</h3>
            <label className="checkbox-row"><input type="checkbox" checked={props.configs.auto_download_covers!=="false"} disabled={savingPreference} onChange={async event=>{setSavingPreference(true);try{await props.onSaveAutoCovers(event.target.checked);}finally{setSavingPreference(false);}}} />Buscar capas ausentes durante a sincronização</label>
            <div className="settings-cover-action"><p>Regenerar recalcula as associações e atualiza as capas automáticas. As ROMs e as capas pessoais não são apagadas.</p><button type="button" className="button-secondary" disabled={props.covers?.running || props.busy} onClick={()=>void props.onRegenerate()}>{props.covers?.running?"Regenerando…":"Regenerar capas"}</button></div>
        </section>
        <details className="panel settings-diagnostics" onToggle={event=>{if(event.currentTarget.open)props.onRefreshLogs();}}><summary>Diagnóstico e logs</summary>
            <div className="panel-toolbar"><button type="button" className="button-quiet" onClick={props.onRefreshLogs}>Atualizar logs</button><button type="button" className="button-quiet" onClick={()=>void props.onClearLogs()}>Limpar logs</button></div>
            <pre className="diagnostic-logs">{props.logs.length?props.logs.join("\n"):"Nenhum log registrado."}</pre>
        </details>
        <section className="panel settings-about">
            <div className="settings-about-header">
                <h3>RomManager {props.version}</h3>
                <span className="about-badge">Licença MIT</span>
            </div>
            <div className="settings-sources-section">
                <h4>Créditos</h4>
                <div className="settings-sources-grid">
                    {credits.map(group => <div className="settings-source-card" key={group.title}>
                        <strong>{group.title}</strong>
                        <p>{group.detail}</p>
                        <ul className="settings-credit-links">{group.links.map(link => <li key={link.url}><a href={link.url} target="_blank" rel="noreferrer">{link.name}</a></li>)}</ul>
                    </div>)}
                </div>
            </div>
            <p className="field-hint settings-disclaimer">Os projetos e serviços citados mantêm suas próprias licenças e condições de uso.</p>
        </section>
    </div>;
}
