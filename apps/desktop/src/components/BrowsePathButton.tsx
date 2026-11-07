import { useState } from "react";
import { choosePath, type PathChoice } from "../lib/nativeFiles";

export function BrowsePathButton({ onSelect, disabled, ...options }: PathChoice & {
  onSelect: (path: string) => void; disabled?: boolean;
}) {
  const [opening, setOpening] = useState(false);
  const [error, setError] = useState("");
  return <span className="browse-path-control">
    <button type="button" className="button-secondary" disabled={disabled || opening}
      aria-label={options.title} onClick={async () => {
        setOpening(true); setError("");
        try { const path = await choosePath(options); if (path !== null) onSelect(path); }
        catch (e) { setError(e instanceof Error ? e.message : String(e)); }
        finally { setOpening(false); }
      }}>{opening ? "Selecionando…" : options.directory ? "Selecionar pasta" : "Selecionar arquivo"}</button>
    {error && <span className="resource-error" role="alert">{error}</span>}
  </span>;
}
