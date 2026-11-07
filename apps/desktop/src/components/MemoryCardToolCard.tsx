import { useEffect, useState } from "react";
import { apiFetch, apiJson, cachedPcsx2Json, invalidatePcsx2Cache } from "../lib/api";

type Tool = { ready: boolean; path: string; version: string };
export function MemoryCardToolCard({ dismissible = false, onChanged }: { dismissible?: boolean; onChanged?: () => void }) {
  const [tool, setTool] = useState<Tool | null>(null);
  const [busy, setBusy] = useState(false), [error, setError] = useState("");
  const [hidden, setHidden] = useState(() => localStorage.getItem("mymcpp-card-hidden") === "true");
  const load = () => cachedPcsx2Json<Tool>("/api/pcsx2/memory-cards/tool").then(setTool).catch(e => setError(e.message));
  useEffect(() => { void load(); }, []);
  if (dismissible && tool?.ready && hidden) return null;
  const change = async (method: "POST" | "DELETE") => {
    setBusy(true); setError("");
    try {
      await apiJson("/api/pcsx2/memory-cards/tool", { method });
      invalidatePcsx2Cache(); await load(); onChanged?.();
    } catch (e) { setError(e instanceof Error ? e.message : "Não foi possível alterar a ferramenta."); }
    finally { setBusy(false); }
  };
  return <section className="panel a4-tool" aria-label="Ferramenta myMCpp">
    <div><h3>myMCpp {tool?.version}</h3><p className="field-hint">{tool?.ready ? "Instalada · criação, importação e exportação de cartões." : "Instale para criar cartões e importar ou exportar saves."}</p>
      {error && <p role="alert" className="field-validation invalid">{error}</p>}
    </div>
    <div className="a4-actions">
      {tool?.ready ? <>
        {!dismissible && <button className="button-secondary" disabled={busy} onClick={() => void apiFetch(`/api/open-folder?path=${encodeURIComponent(tool.path.replace(/[\\/][^\\/]+$/, ""))}`).catch(e => setError(e.message))}>Abrir pasta</button>}
        {!dismissible && <button className="button-secondary" disabled={busy} onClick={() => void change("DELETE")}>{busy ? "Aguarde…" : "Desinstalar ferramenta"}</button>}
        {dismissible && <button className="button-quiet" disabled={busy} aria-label="Ocultar card da ferramenta" onClick={() => { localStorage.setItem("mymcpp-card-hidden", "true"); setHidden(true); }}>Ocultar</button>}
      </> : <button className="button-secondary" disabled={busy || !tool} onClick={() => void change("POST")}>{busy ? "Instalando…" : "Instalar myMCpp"}</button>}
    </div>
  </section>;
}
