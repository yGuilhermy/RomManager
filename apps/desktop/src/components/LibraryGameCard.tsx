import type { InstalledGame } from "../types";
import { CoverImage } from "./CoverImage";
import { formatBytes } from "../lib/format";

interface Props { game:InstalledGame; onOpen:()=>void; onLaunch:()=>void; onDelete:()=>void; busy?:boolean; }
export function LibraryGameCard({game,onOpen,onLaunch,onDelete,busy=false}:Props) {
    return <article className="library-game-card">
        <button type="button" className="library-game-cover" onClick={onOpen} aria-label={`Detalhes de ${game.title}`}>
            <CoverImage src={game.cover_url} title={game.title} region={game.region || undefined} />
        </button>
        <div className="library-game-info">
            <h3><button type="button" className="card-open" onClick={onOpen}>{game.title}</button></h3>
            <p className="library-game-meta"><span>{game.format.toUpperCase()}</span><span>{formatBytes(game.file_size)}</span><span>{game.region || "Região desconhecida"}</span></p>
            {game.serial && <p className="library-game-serial">{game.serial}</p>}
            <p className="library-game-path" title={game.file_path}>{game.file_path}</p>
        </div>
        <div className="library-game-actions">
            <button type="button" className="library-action library-action-launch" disabled={busy} onClick={onLaunch}>Iniciar PCSX2</button>
            <button type="button" className="library-action library-action-delete" onClick={onDelete}>Deletar</button>
        </div>
    </article>;
}
