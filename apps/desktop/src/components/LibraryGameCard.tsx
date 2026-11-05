import type { InstalledGame } from "../types";
import { CoverImage } from "./CoverImage";
import { formatBytes } from "../lib/format";

interface Props { game:InstalledGame; onOpen:()=>void; onLaunch:()=>void; onDelete:()=>void; busy?:boolean; }
export function LibraryGameCard({game,onOpen,onLaunch,onDelete,busy=false}:Props) {
    const isMissing = Boolean(game.is_missing);
    const discMatch = game.title.match(/(?:disc|disco)\s*(\d+)/i);
    const discLabel = discMatch ? `Disco ${discMatch[1]}` : null;

    return <article className={`library-game-card${isMissing ? " is-missing" : ""}`}>
        <button type="button" className="library-game-cover" onClick={onOpen} aria-label={`Detalhes de ${game.title}`}>
            <CoverImage src={game.cover_url} title={game.title} region={game.region || undefined} />
        </button>
        <div className="library-game-info">
            <h3><button type="button" className="card-open" onClick={onOpen}>{game.title}</button></h3>
            <div className="card-badges-row">
                {isMissing && <span className="card-badge-chip card-badge-danger" title="Arquivo não encontrado na pasta">Ausente</span>}
                {discLabel && <span className="card-badge-chip card-badge-source">{discLabel}</span>}
                <span className="card-badge-chip card-badge-format">{game.format.toUpperCase()}</span>
                {game.region && <span className="card-badge-chip card-badge-region">{game.region}</span>}
            </div>
            <p className="library-game-meta"><span>{formatBytes(game.file_size)}</span>{game.serial && <span>{game.serial}</span>}</p>
            <p className="library-game-path" title={game.file_path}>{game.file_path}</p>
        </div>
        <div className="library-game-actions">
            <button
                type="button"
                className="library-action library-action-launch"
                disabled={busy || isMissing}
                onClick={onLaunch}
                title={isMissing ? "Arquivo não encontrado na pasta de ROMs" : "Iniciar no PCSX2"}
            >
                {isMissing ? "Arquivo ausente" : "Iniciar PCSX2"}
            </button>
            <button type="button" className="library-action library-action-delete" onClick={onDelete}>Deletar</button>
        </div>
    </article>;
}
