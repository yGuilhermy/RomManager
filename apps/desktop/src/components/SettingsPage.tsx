import { MemoryCardToolCard } from "./MemoryCardToolCard";
import { BrowsePathButton } from "./BrowsePathButton";
import { apiFetch } from "../lib/api";
import { useState } from "react";
import type { ValidationResult,CoverJob } from "../types";
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
            <p>Gerenciador local de ROMs de PlayStation 2 com catálogo integrado, download inteligente e execução direta no PCSX2.</p>
            <p className="field-hint">API local atendendo em <code>127.0.0.1:3000</code>.</p>

            <div className="settings-sources-section">
                <h4>Origem dos dados e acervos de terceiros</h4>
                <div className="settings-sources-grid">
                    <div className="settings-source-card">
                        <strong>Catálogo e ROMs CHD</strong>
                        <p>Internet Archive (coleções públicas 1G1R para preservação digital do acervo de PS2).</p>
                    </div>
                    <div className="settings-source-card">
                        <strong>Capas e Encartes</strong>
                        <p>Repositórios abertos Libretro Thumbnails e ps2-covers (xlenore) indexados por serial e título.</p>
                    </div>
                    <div className="settings-source-card">
                        <strong>Pesquisa de Traduções</strong>
                        <p>CDRomance (consulta sob demanda de títulos com localização em português e modificações comunitárias).</p>
                    </div>
                    <div className="settings-source-card">
                        <strong>Sinopses e Compatibilidade</strong>
                        <p>Wikipédia (Wikimedia API) para descrições históricas e base oficial comunitária da PCSX2 Wiki.</p>
                    </div>
                </div>
            </div>

            <p className="field-hint settings-disclaimer">Todas as marcas registradas, títulos e materiais gráficos pertencem aos seus respectivos detentores de direitos. O RomManager não comercializa nem hospeda arquivos proprietários ou BIOS de console.</p>
        </section>
    </div>;
}
