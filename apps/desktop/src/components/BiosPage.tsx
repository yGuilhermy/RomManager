import React, { useState, useEffect, useCallback } from "react";
import { cachedPcsx2Json, invalidatePcsx2Cache, apiFetch } from "../lib/api";
import { BiosInfo } from "../types";

interface BiosPageProps {
    onShowToast: (message: string, type?: "success" | "error" | "info") => void;
}

export function BiosPage({ onShowToast }: BiosPageProps) {
    const [biosList, setBiosList] = useState<BiosInfo[]>([]);
    const [notice, setNotice] = useState(() => sessionStorage.getItem("bios-notice-dismissed") !== "1");
    const [loading, setLoading] = useState(false);
    const [importPath, setImportPath] = useState("");
    const [isImporting, setIsImporting] = useState(false);

    const loadBios = useCallback(async () => {
        setLoading(true);
        try {
            const data = await cachedPcsx2Json<BiosInfo[]>("/api/pcsx2/bios");
            setBiosList(data);
        } catch {
            onShowToast("Erro ao carregar lista de BIOS do PCSX2.", "error");
        } finally {
            setLoading(false);
        }
    }, [onShowToast]);

    useEffect(() => {
        loadBios();
    }, [loadBios]);

    const handleSelectBios = async (path: string) => {
        try {
            const res = await apiFetch("/api/pcsx2/bios/select", {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({ bios_path: path })
            });
            if (!res.ok) {
                const errText = await res.text();
                onShowToast(errText || "Falha ao selecionar BIOS ativa.", "error");
                return;
            }
            onShowToast("BIOS selecionada como padrão do emulador.", "success");
            loadBios();
        } catch {
            onShowToast("Erro de rede ao selecionar BIOS.", "error");
        }
    };

    const handleImportBios = async (e: React.FormEvent) => {
        e.preventDefault();
        const cleanPath = importPath.trim().replace(/^["']|["']$/g, "").trim();
        if (!cleanPath) {
            onShowToast("Informe o caminho do arquivo de BIOS no disco.", "error");
            return;
        }

        setIsImporting(true);
        try {
            const res = await apiFetch("/api/pcsx2/bios/import", {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({ source_path: cleanPath })
            });

            if (!res.ok) {
                const err = await res.text();
                onShowToast(err || "Falha na importação da BIOS.", "error");
                return;
            }

            const importedData: any = await res.json();
            if (importedData.count && importedData.count > 1) {
                onShowToast(`${importedData.count} arquivos de BIOS importados com sucesso a partir do pacote ZIP.`, "success");
            } else {
                const name = importedData.filename || importedData.items?.[0]?.filename || "bios.bin";
                const model = importedData.model || importedData.items?.[0]?.model || "PlayStation 2";
                onShowToast(`BIOS '${name}' (${model}) importada com sucesso.`, "success");
            }
            setImportPath("");
            loadBios();
        } catch {
            onShowToast("Erro ao comunicar com o servidor de importação.", "error");
        } finally {
            setIsImporting(false);
        }
    };

    const sortedBiosList = [...biosList].sort((a, b) => {
        if (a.is_active === b.is_active) {
            return a.filename.localeCompare(b.filename);
        }
        return a.is_active ? -1 : 1;
    });

    return (
        <div className="bios-page">
            <header className="bios-page-header">
                <div className="page-heading">
                    <h2>BIOS</h2>

                </div>
            </header>

            {notice && <div className="bios-notice-box panel-header"><span>Importe a BIOS do seu PS2. O RomManager não baixa BIOS.</span><button className="button-quiet" aria-label="Fechar aviso de BIOS" onClick={() => { setNotice(false); sessionStorage.setItem("bios-notice-dismissed", "1"); }}>×</button></div>}

            <section className="panel">
                <h3>Importar BIOS</h3>
                <p className="field-hint">Arquivo .bin, .rom ou pacote <code>.zip</code>.</p>
                <form onSubmit={handleImportBios} className="bios-import-form">
                    <input
                        type="text"
                        value={importPath}
                        onChange={e => setImportPath(e.target.value)}
                        placeholder="Ex: C:\MeusArquivos\bios_pack.zip ou SCPH-39001.bin"
                        aria-label="Caminho do arquivo de BIOS"
                    />
                    <button
                        type="submit"
                        disabled={isImporting}
                        className="button-primary"
                    >
                        {isImporting ? "Importando..." : "Importar BIOS"}
                    </button>
                </form>
            </section>

            <section className="panel">
                <div className="panel-header">
                    <h3>BIOS disponíveis</h3>
                    <span className="field-hint">
                        <button className="button-quiet" disabled={loading} onClick={() => { invalidatePcsx2Cache(); void loadBios(); }}>Atualizar</button>
                    </span>
                </div>

                {loading ? (
                    <div className="emulator-loading-state">
                        Carregando arquivos...
                    </div>
                ) : biosList.length === 0 ? (
                    <div className="emulator-empty-state">
                        Nenhum arquivo de BIOS foi encontrado no diretório do emulador. Importe sua BIOS acima para poder executar jogos no PCSX2.
                    </div>
                ) : (
                    <div className="bios-list">
                        {sortedBiosList.map((bios) => (
                            <article
                                key={bios.path}
                                className={`bios-card ${bios.is_active ? "active" : ""} ${bios.is_duplicate ? "duplicate" : ""}`}
                            >
                                <div className="bios-card-info">
                                    <div className="bios-card-header">
                                        <span className="bios-card-model">{bios.model}</span>
                                        <span className="badge-pill region">{bios.region}</span>
                                        <span className="badge-pill neutral">{bios.version}</span>
                                        {bios.is_active && (
                                            <span className="badge-pill active">Ativa</span>
                                        )}
                                        {bios.is_duplicate && (
                                            <span className="badge-pill duplicate">Duplicada</span>
                                        )}
                                    </div>
                                    <div className="bios-card-meta">
                                        Arquivo: <code>{bios.filename}</code> • Tamanho: {(bios.size / (1024 * 1024)).toFixed(2)} MB
                                    </div>

                                </div>

                                <div className="bios-card-actions">
                                    {!bios.is_active && (
                                        <button
                                            type="button"
                                            className="button-secondary"
                                            onClick={() => handleSelectBios(bios.path)}
                                        >
                                            Usar BIOS
                                        </button>
                                    )}
                                </div>
                            </article>
                        ))}
                    </div>
                )}
            </section>
            <section className="panel"><h3>Como extrair a BIOS do seu PS2</h3><ol className="bios-dump-steps"><li>Prepare um pendrive FAT32 e baixe o biosdrain pelo guia oficial.</li><li>No PS2, abra o uLaunchELF por um método compatível com seu modelo, como FreeMcBoot.</li><li>Execute o biosdrain no console e aguarde o dump terminar no pendrive.</li><li>Copie os arquivos para o PC e importe a BIOS acima.</li></ol><a href="https://pcsx2.net/docs/setup/bios/" target="_blank" rel="noreferrer">Guia oficial do PCSX2 ↗</a></section>
        </div>
    );
}
