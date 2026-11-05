import { useEffect, useRef } from "react";

interface Props {
    game: { title: string; filePath?: string };
    busy: boolean;
    onClose: () => void;
    onConfirm: () => void;
}

export function DeleteGameDialog({ game, busy, onClose, onConfirm }: Props) {
    const dialog = useRef<HTMLDialogElement>(null);
    const cancelButton = useRef<HTMLButtonElement>(null);
    useEffect(() => {
        const element = dialog.current;
        const previous = document.activeElement as HTMLElement | null;
        element?.showModal();
        cancelButton.current?.focus();
        return () => { element?.close(); if (previous?.isConnected) previous.focus(); };
    }, []);
    return <dialog ref={dialog} className="delete-game-dialog" aria-labelledby="delete-game-title"
        aria-describedby="delete-game-description" aria-busy={busy}
        onCancel={event => { event.preventDefault(); if (!busy) onClose(); }}>
        <h2 id="delete-game-title">Excluir {game.title}?</h2>
        <p id="delete-game-description">A ROM será apagada permanentemente do disco, sem passar pela Lixeira. As capas seráo preservadas.</p>
        {game.filePath && <p className="delete-game-path">{game.filePath}</p>}
        <div className="delete-game-actions">
            <button ref={cancelButton} type="button" className="sync-btn secondary" disabled={busy} onClick={onClose}>Manter jogo</button>
            <button type="button" className="sync-btn danger" disabled={busy} onClick={onConfirm}>{busy ? "Excluindo…" : "Excluir ROM do disco"}</button>
        </div>
    </dialog>;
}
