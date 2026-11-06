import React, { useState, useEffect, useCallback } from "react";
import { apiFetch, cachedPcsx2Json, invalidatePcsx2Cache } from "../lib/api";
import {
    Pcsx2Installation,
    Pcsx2Profile,
    Pcsx2Diagnostic,
    ManagedRelease,
    CoversExportResult,
    formatPcsx2Version
} from "../types";

interface EmulatorPageProps {
    onShowToast: (message: string, type?: "success" | "error" | "info") => void;
}

export function EmulatorPage({ onShowToast }: EmulatorPageProps) {
    const initialSubTab = typeof window !== "undefined" ? new URLSearchParams(window.location.search).get("subtab") : null;
    const [subTab, setSubTab] = useState<"installations" | "profiles">(
        initialSubTab === "profiles" ? initialSubTab : "installations"
    );

    const [installations, setInstallations] = useState<Pcsx2Installation[]>([]);
    const [releases, setReleases] = useState<ManagedRelease[]>([]);
    const [profiles, setProfiles] = useState<Pcsx2Profile[]>([]);
    const [diagnostic, setDiagnostic] = useState<Pcsx2Diagnostic | null>(null);
    const [releaseTag, setReleaseTag] = useState("");
    const [channel, setChannel] = useState("stable");
    const [releaseVersion, setReleaseVersion] = useState("");
    const [showProfileForm, setShowProfileForm] = useState(false);
    const [loading, setLoading] = useState(false);
    const [isRefreshingReleases, setIsRefreshingReleases] = useState(false);

    const [isExportingCovers, setIsExportingCovers] = useState(false);
    const [exportOverwrite, setExportOverwrite] = useState(false);
    const [coversResult, setCoversResult] = useState<CoversExportResult | null>(null);

    const [isInstallingRelease, setIsInstallingRelease] = useState<string | null>(null);

    const [editingProfile, setEditingProfile] = useState<Pcsx2Profile | null>(null);
    const [newProfileName, setNewProfileName] = useState("");
    const [newProfileExe, setNewProfileExe] = useState("");
    const [newProfileDataDir, setNewProfileDataDir] = useState("");
    const [newProfileRenderer, setNewProfileRenderer] = useState("Vulkan");
    const [newProfileAspect, setNewProfileAspect] = useState("16:9");
    const [newProfileFullscreen, setNewProfileFullscreen] = useState(false);
    const [customExeInput, setCustomExeInput] = useState("");
    const [selectedInstPreset, setSelectedInstPreset] = useState("custom");

    const loadData = useCallback(async () => {
        setLoading(true);
        try {
            const [instData, relData, profData, diagData] = await Promise.all([
                cachedPcsx2Json<Pcsx2Installation[]>("/api/pcsx2/detect"),
                cachedPcsx2Json<ManagedRelease[]>("/api/pcsx2/releases"),
                cachedPcsx2Json<Pcsx2Profile[]>("/api/pcsx2/profiles"),
                cachedPcsx2Json<Pcsx2Diagnostic>("/api/pcsx2/diagnostic"),
            ]);
            setInstallations(instData);
            setReleases(relData);
            setProfiles(profData);
            setDiagnostic(diagData);
        } catch {
            onShowToast("Erro ao carregar dados do emulador PCSX2.", "error");
        } finally {
            setLoading(false);
        }
    }, [onShowToast]);

    useEffect(() => {
        loadData();
    }, [loadData]);

    const activeProfile = profiles.find(p => p.is_active);
    const activeExe = activeProfile?.executable_path || diagnostic?.executable_path || "";
    const activeInstallation = installations.find(i => i.path.toLowerCase() === activeExe.toLowerCase());

    const handleSelectVersion = async (executablePath: string, dataDir?: string) => {
        try {
            const res = await apiFetch("/api/pcsx2/version/select", {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({ executable_path: executablePath, data_dir: dataDir })
            });
            if (!res.ok) {
                const errText = await res.text();
                onShowToast(errText || "Falha ao definir versão ativa.", "error");
                return;
            }
            onShowToast("Versão do PCSX2 ativada com sucesso.", "success");
            loadData();
        } catch {
            onShowToast("Erro de rede ao trocar versão ativa.", "error");
        }
    };

    const handleRefreshReleases = async () => {
        setIsRefreshingReleases(true);
        try {
            const relData = await cachedPcsx2Json<ManagedRelease[]>("/api/pcsx2/releases?refresh=true");
            setReleases(relData);
            onShowToast("Versões oficiais do PCSX2 atualizadas diretamente do GitHub.", "success");
        } catch {
            onShowToast("Erro ao consultar versões atualizadas do GitHub.", "error");
        } finally {
            setIsRefreshingReleases(false);
        }
    };

    const handleSaveCustomExe = async (e: React.FormEvent) => {
        e.preventDefault();
        const clean = customExeInput.trim().replace(/^["']|["']$/g, "").trim();
        if (!clean) {
            onShowToast("Informe o caminho do executável customizado.", "error");
            return;
        }
        await handleSelectVersion(clean);
        setCustomExeInput("");
    };

    const handleSelectReleaseVersion = (rel: ManagedRelease) => {
        if (rel.executable_path) {
            handleSelectVersion(rel.executable_path, rel.data_dir || undefined);
            return;
        }
        onShowToast("Instale esta versão para usá-la.", "info");
    };

    const handleActivateProfile = async (id: string) => {
        try {
            const res = await apiFetch(`/api/pcsx2/profiles/${id}/activate`, { method: "POST" });
            if (!res.ok) {
                const errText = await res.text();
                onShowToast(errText || "Falha ao ativar perfil.", "error");
                return;
            }
            onShowToast("Perfil ativado com sucesso.", "success");
            loadData();
        } catch {
            onShowToast("Erro de rede ao ativar perfil.", "error");
        }
    };

    const handleDeleteProfile = async (id: string) => {
        try {
            const res = await apiFetch(`/api/pcsx2/profiles/${id}`, { method: "DELETE" });
            if (!res.ok) {
                const errText = await res.text();
                onShowToast(errText || "Falha ao excluir perfil.", "error");
                return;
            }
            onShowToast("Perfil excluído com sucesso.", "info");
            loadData();
        } catch {
            onShowToast("Erro de rede ao excluir perfil.", "error");
        }
    };

    const handleSaveProfile = async (e: React.FormEvent) => {
        e.preventDefault();
        const cleanName = newProfileName.trim();
        const cleanExe = newProfileExe.trim().replace(/^["']|["']$/g, "").trim();
        const cleanDataDir = newProfileDataDir.trim().replace(/^["']|["']$/g, "").trim();

        if (!cleanName) {
            onShowToast("Informe o nome do perfil.", "error");
            return;
        }
        if (!cleanExe) {
            onShowToast("Informe o caminho do executável do PCSX2.", "error");
            return;
        }

        const profilePayload: Pcsx2Profile = editingProfile ? {
            ...editingProfile,
            name: cleanName,
            executable_path: cleanExe,
            data_dir: cleanDataDir,
            renderer: newProfileRenderer,
            aspect_ratio: newProfileAspect,
            fullscreen: newProfileFullscreen,
            updated_at: new Date().toISOString()
        } : {
            id: `prof_${Date.now()}`,
            name: cleanName,
            executable_path: cleanExe,
            data_dir: cleanDataDir,
            bios_path: null,
            renderer: newProfileRenderer,
            aspect_ratio: newProfileAspect,
            fullscreen: newProfileFullscreen,
            is_active: profiles.length === 0,
            created_at: new Date().toISOString(),
            updated_at: new Date().toISOString()
        };

        try {
            const res = await apiFetch("/api/pcsx2/profiles", {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify(profilePayload)
            });
            if (!res.ok) {
                const errText = await res.text();
                onShowToast(errText || "Falha ao salvar perfil.", "error");
                return;
            }
            onShowToast("Perfil salvo com sucesso.", "success");
            setEditingProfile(null);
            setShowProfileForm(false);
            setNewProfileName("");
            setNewProfileExe("");
            setNewProfileDataDir("");
            loadData();
        } catch {
            onShowToast("Erro ao salvar perfil.", "error");
        }
    };

    const handleInstallRelease = async (version: string) => {
        setIsInstallingRelease(version);
        try {
            const res = await apiFetch("/api/pcsx2/install", {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({ version })
            });
            if (!res.ok) {
                const err = await res.text();
                onShowToast(err || "Falha na instalação/atualização.", "error");
                return;
            }
            onShowToast(`PCSX2 ${version} instalado e vinculado com sucesso.`, "success");
            loadData();
        } catch (error) {
            onShowToast(error instanceof Error ? error.message : "Erro durante instalação.", "error");
        } finally {
            setIsInstallingRelease(null);
        }
    };

    const handleExportCovers = async () => {
        setIsExportingCovers(true);
        try {
            const res = await apiFetch("/api/pcsx2/covers/export", {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({ overwrite: exportOverwrite })
            });
            if (!res.ok) {
                const err = await res.text();
                onShowToast(err || "Falha ao exportar capas para o PCSX2.", "error");
                return;
            }
            const data: CoversExportResult = await res.json();
            setCoversResult(data);
            onShowToast(`Exportação concluída: ${data.exported} exportadas, ${data.preserved} preservadas.`, "success");
        } catch {
            onShowToast("Erro de comunicação ao exportar capas.", "error");
        } finally {
            setIsExportingCovers(false);
        }
    };

    const startEditingProfile = (prof: Pcsx2Profile) => {
        setShowProfileForm(true);
        setEditingProfile(prof);
        setNewProfileName(prof.name);
        setNewProfileExe(prof.executable_path);
        setNewProfileDataDir(prof.data_dir || "");
        setNewProfileRenderer(prof.renderer);
        setNewProfileAspect(prof.aspect_ratio);
        setNewProfileFullscreen(prof.fullscreen);
        setSelectedInstPreset(prof.executable_path);
    };

    const available = releases.filter(r => channel === "custom" || r.channel === channel).sort((a,b) => b.published_at.localeCompare(a.published_at));
    const selected = available.find(r => r.version === releaseVersion) || available[0];
    return <div className="emulator-page emulator-compact">
        <header className="panel-header emulator-heading"><div><h2>Instalação e perfis</h2><p>PCSX2 · versões e configurações de jogo</p></div><button className="button-secondary" disabled={loading} onClick={() => { invalidatePcsx2Cache(); void loadData(); }}>Atualizar</button></header>
        {diagnostic?.is_running && <p role="status">PCSX2 aberto · alterações bloqueadas</p>}
        <nav className="emulator-tabs-nav" aria-label="Emulador">
            <button className={`emulator-tab-btn ${subTab === "installations" ? "active" : ""}`} onClick={() => setSubTab("installations")}>Instalações</button>
            <button className={`emulator-tab-btn ${subTab === "profiles" ? "active" : ""}`} onClick={() => setSubTab("profiles")}>Perfis e diagnóstico</button>
        </nav>
        {loading && !diagnostic && <p role="status">Carregando…</p>}
        {subTab === "installations" ? <div className="emulator-tab-content installation-layout">
            <section className="panel installation-current"><div className="installation-summary"><span className="emulator-device-mark" aria-hidden="true"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5"><rect x="3" y="4" width="18" height="13" rx="2"/><path d="M8 21h8m-4-4v4M7 9h4m-2-2v4m7-2h1"/></svg></span><div><h3>Instalação ativa</h3><strong>{activeInstallation ? formatPcsx2Version(activeInstallation.version) : "PCSX2"}</strong></div><span className={`badge-pill ${activeExe ? "active" : "neutral"}`}>{activeExe ? "Selecionada" : "Não configurada"}</span></div>
                <label className="emulator-form-field">PCSX2<select id="emulator-version-select" value={activeExe} disabled={diagnostic?.is_running} onChange={e => { const i = installations.find(i => i.path === e.target.value); void handleSelectVersion(e.target.value, i?.data_dir); }}>
                    <option value="" disabled>Selecionar instalação</option>
                    {installations.map(i => <option key={i.path} value={i.path}>{formatPcsx2Version(i.version)} · {i.path}</option>)}
                    {activeExe && !activeInstallation && <option value={activeExe}>{activeExe}</option>}
                </select></label>
                <details><summary>Usar executável local</summary><form className="bios-import-form" onSubmit={handleSaveCustomExe}><input aria-label="Executável local" value={customExeInput} onChange={e => setCustomExeInput(e.target.value)} placeholder="C:\PCSX2\pcsx2-qt.exe"/><button className="button-secondary" disabled={diagnostic?.is_running}>Usar</button></form></details>
            </section>
            <section className="panel installation-download"><div className="panel-header"><h3>Baixar PCSX2</h3><button className="button-quiet" disabled={isRefreshingReleases} onClick={handleRefreshReleases}>{isRefreshingReleases ? "Consultando…" : "Atualizar versões"}</button></div>
                <div className="emulator-form-row"><label className="emulator-form-field">Canal<select value={channel} onChange={e => { setChannel(e.target.value); setReleaseVersion(""); }}><option value="stable">Estável</option><option value="nightly">Nightly</option><option value="custom">Escolher versão</option></select></label>
                <label className="emulator-form-field">Versão<select value={selected?.version || ""} onChange={e => setReleaseVersion(e.target.value)}>{available.map((r,i) => <option key={r.version} value={r.version}>{formatPcsx2Version(r.version)}{i === 0 && channel !== "custom" ? " · mais recente" : ""}{r.installed ? " · instalada" : ""}</option>)}</select></label></div>
                {channel === "custom" && <form className="bios-import-form" onSubmit={async e => { e.preventDefault(); if (!releaseTag.trim()) return; setIsRefreshingReleases(true); try { const result = await cachedPcsx2Json<ManagedRelease[]>(`/api/pcsx2/releases?tag=${encodeURIComponent(releaseTag.trim())}`); setReleases(result); setReleaseVersion(result[0]?.version || ""); } catch (error) { onShowToast(error instanceof Error ? error.message : "Versão não encontrada.", "error"); } finally { setIsRefreshingReleases(false); } }}><input aria-label="Tag da versão no GitHub" placeholder="Tag do GitHub, ex.: v2.2.0" value={releaseTag} onChange={e => setReleaseTag(e.target.value)}/><button className="button-secondary" disabled={isRefreshingReleases}>Buscar versão</button></form>}
                {!selected ? <p>Lista indisponível. Atualize as versões para consultar o GitHub.</p> : <div className="panel-header release-install-footer"><span>{selected.file_size ? `${(selected.file_size / 1048576).toFixed(0)} MB · ` : ""}GitHub / PCSX2</span><button className="button-primary" disabled={!!isInstallingRelease || diagnostic?.is_running} onClick={() => selected.installed ? handleSelectReleaseVersion(selected) : handleInstallRelease(selected.version)}>{isInstallingRelease ? "Instalando…" : selected.installed ? "Usar versão" : "Baixar e instalar"}</button></div>}
            </section>
            <section className="panel installation-covers"><div className="panel-header"><div><h3>Capas do PCSX2</h3><p className="field-hint">Use as capas da biblioteca no emulador( ).</p></div><button className="button-secondary" onClick={handleExportCovers} disabled={isExportingCovers || diagnostic?.is_running}>{isExportingCovers ? "Copiando…" : "Sincronizar capas"}</button></div><label className="checkbox-row"><input type="checkbox" checked={exportOverwrite} onChange={e => setExportOverwrite(e.target.checked)}/>Substituir capas existentes</label>{coversResult && <p role="status">{coversResult.exported} copiadas · {coversResult.preserved} preservadas · {coversResult.errors} erros</p>}</section>
        </div> : <div className="emulator-tab-content profiles-layout">
            <section className="panel"><div className="panel-header"><h3>Perfis</h3><button className="button-secondary" disabled={diagnostic?.is_running} onClick={() => { setEditingProfile(null); setNewProfileName(""); setNewProfileExe(activeExe); setNewProfileDataDir(activeProfile?.data_dir || ""); setNewProfileRenderer("Vulkan"); setNewProfileAspect("4:3"); setNewProfileFullscreen(false); setSelectedInstPreset("custom"); setShowProfileForm(true); }}>Novo perfil</button></div>
            {!profiles.length && <p>Nenhum perfil cadastrado.</p>}
            {profiles.map(p => <article className="emulator-profile-row" key={p.id}><div><strong>{p.name}</strong>{p.is_active && <span className="badge-pill active">Ativo</span>}<p>{p.renderer} · {p.aspect_ratio}{p.fullscreen ? " · Tela cheia" : ""}</p><details><summary>Caminhos</summary><p>{p.executable_path}</p><p>{p.data_dir}</p></details></div><div className="emulator-profile-actions"><button className="button-secondary" disabled={diagnostic?.is_running} onClick={() => startEditingProfile(p)}>Editar</button>{!p.is_active && <><button className="button-secondary" disabled={diagnostic?.is_running} onClick={() => handleActivateProfile(p.id)}>Ativar</button><button className="button-quiet danger-text" disabled={diagnostic?.is_running} onClick={() => handleDeleteProfile(p.id)}>Excluir</button></>}</div></article>)}
            {showProfileForm && <fieldset disabled={diagnostic?.is_running} className="profile-editor">                        <form onSubmit={handleSaveProfile} className="emulator-profile-form">
                            <h3>{editingProfile ? `Editar Perfil: ${editingProfile.name}` : "Novo perfil"}</h3>
                            <div className="emulator-form-row">
                                <div className="emulator-form-field">
                                    <label>Nome do Perfil</label>
                                    <input
                                        type="text"
                                        value={newProfileName}
                                        onChange={e => setNewProfileName(e.target.value)}
                                        placeholder="Ex: Vulkan Performance"
                                    />
                                </div>
                                <div className="emulator-form-field">
                                    <label>Renderizador Gráfico</label>
                                    <select
                                        value={newProfileRenderer}
                                        onChange={e => setNewProfileRenderer(e.target.value)}
                                    >
                                        <option value="Vulkan">Vulkan</option>
                                        <option value="Direct3D11">Direct3D 11</option>
                                        <option value="Direct3D12">Direct3D 12</option>
                                        <option value="OpenGL">OpenGL</option>
                                    </select>
                                </div>
                            </div>
                            <div className="emulator-form-field">
                                <label>Instalação</label>
                                <select
                                    value={selectedInstPreset}
                                    onChange={e => {
                                        const val = e.target.value;
                                        setSelectedInstPreset(val);
                                        if (val !== "custom") {
                                            setNewProfileExe(val);
                                            const match = installations.find(i => i.path === val);
                                            if (match && match.data_dir) {
                                                setNewProfileDataDir(match.data_dir);
                                            }
                                        }
                                    }}
                                >
                                    <option value="custom">-- Personalizado / Manual --</option>
                                    {installations.map(inst => (
                                        <option key={inst.path} value={inst.path}>
                                            {formatPcsx2Version(inst.version)} ({inst.is_managed ? "Gerenciado" : inst.is_portable ? "Portátil" : "Sistema"})
                                        </option>
                                    ))}
                                </select>
                            </div>
                            <div className="emulator-form-field">
                                <label>Caminho do Executável</label>
                                <input
                                    type="text"
                                    value={newProfileExe}
                                    onChange={e => {
                                        setNewProfileExe(e.target.value);
                                        setSelectedInstPreset("custom");
                                    }}
                                    placeholder="C:\...\pcsx2-qt.exe"
                                />
                            </div>
                            <div className="emulator-form-field">
                                <label>Pasta de dados (opcional)</label>
                                <input
                                    type="text"
                                    value={newProfileDataDir}
                                    onChange={e => setNewProfileDataDir(e.target.value)}
                                    placeholder="Ex: C:\Users\...\Documents\PCSX2 ou pasta portátil"
                                />
                            </div>
                            <div className="emulator-form-inline-options">
                                <div className="emulator-form-field">
                                    <label>Proporção de Tela</label>
                                    <select
                                        value={newProfileAspect}
                                        onChange={e => setNewProfileAspect(e.target.value)}
                                    >
                                        <option value="16:9">16:9 (Widescreen)</option>
                                        <option value="4:3">4:3 (Padrão Original)</option>
                                    </select>
                                </div>
                                <label className="checkbox-row">
                                    <input
                                        type="checkbox"
                                        checked={newProfileFullscreen}
                                        onChange={e => setNewProfileFullscreen(e.target.checked)}
                                    />
                                    <span>Iniciar em Tela Cheia</span>
                                </label>
                            </div>
                            <div className="emulator-form-footer">
                                {editingProfile && (
                                    <button
                                        type="button"
                                        className="button-secondary"
                                        onClick={() => { setEditingProfile(null); setShowProfileForm(false); }}
                                    >
                                        Cancelar
                                    </button>
                                )}
                                <button type="submit" className="button-primary">
                                    {editingProfile ? "Atualizar Perfil" : "Salvar Perfil"}
                                </button>
                            </div>
                        </form></fieldset>}</section>
            {diagnostic && <section className="panel emulator-diagnostic"><div className="panel-header"><h3>Diagnóstico</h3><button className="button-quiet" disabled={loading} onClick={() => { invalidatePcsx2Cache(); void loadData(); }}>Verificar agora</button></div>
                <ul className="diagnostic-checks">{[["Executável",diagnostic.executable_valid],["Pasta de dados",diagnostic.data_dir_valid && diagnostic.data_dir_writable],["BIOS ativa",diagnostic.active_bios_valid],["Pasta de ROMs",diagnostic.roms_dir_valid]].map(([label,ok]) => <li key={String(label)}><span>{label}</span><span className={ok ? "text-success" : "text-danger"}>{ok ? "OK" : "Verificar"}</span></li>)}</ul>
                {[...new Set([...diagnostic.errors,...diagnostic.warnings])].map(message => <p key={message}>{message}</p>)}</section>}
        </div>}
    </div>;
}
