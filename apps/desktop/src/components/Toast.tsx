import { useState, useCallback, useEffect, useRef } from "react";

export type ToastType = "success" | "error" | "info" | "warning";

interface ToastItem {
    id: number;
    message: string;
    type: ToastType;
    exiting: boolean;
}

let _addToast: ((message: string, type: ToastType) => void) | null = null;

export function showToast(message: string, type: ToastType = "info") {
    if (_addToast) _addToast(message, type);
}

let nextId = 0;

const ICONS: Record<ToastType, string> = {
    success: `<svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="2"><circle cx="10" cy="10" r="8"/><path d="M6.5 10.5l2.5 2.5 4-5" stroke-linecap="round" stroke-linejoin="round"/></svg>`,
    error:   `<svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="2"><circle cx="10" cy="10" r="8"/><path d="M10 6v4M10 14h.01" stroke-linecap="round"/></svg>`,
    warning: `<svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="2"><path d="M10 3L18 17H2L10 3z" stroke-linejoin="round"/><path d="M10 9v3M10 14h.01" stroke-linecap="round"/></svg>`,
    info:    `<svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="2"><circle cx="10" cy="10" r="8"/><path d="M10 9v5M10 6h.01" stroke-linecap="round"/></svg>`,
};

const DURATIONS: Record<ToastType, number> = {
    success: 4000,
    info:    4000,
    warning: 5000,
    error:   6000,
};

export function ToastContainer() {
    const [toasts, setToasts] = useState<ToastItem[]>([]);
    const timers = useRef<Map<number, ReturnType<typeof setTimeout>>>(new Map());

    const removeToast = useCallback((id: number) => {
        setToasts(prev => prev.map(t => t.id === id ? { ...t, exiting: true } : t));
        const previous = timers.current.get(id);
        if (previous) clearTimeout(previous);
        const exitTimer = setTimeout(() => {
            setToasts(prev => prev.filter(t => t.id !== id));
            timers.current.delete(id);
        }, 300);
        timers.current.set(id, exitTimer);
    }, []);

    const addToast = useCallback((message: string, type: ToastType) => {
        const id = nextId++;
        setToasts(prev => [...prev, { id, message, type, exiting: false }]);
        const timer = setTimeout(() => removeToast(id), DURATIONS[type]);
        timers.current.set(id, timer);
    }, [removeToast]);

    useEffect(() => {
        _addToast = addToast;
        return () => { _addToast = null; };
    }, [addToast]);

    useEffect(() => {
        return () => {
            timers.current.forEach(t => clearTimeout(t));
        };
    }, []);


    return (
        <div className="toast-container" aria-live="polite" aria-atomic="false">
            {toasts.map(t => (
                <div
                    key={t.id}
                    className={`toast toast-${t.type}${t.exiting ? " toast-exit" : ""}`}
                    onClick={() => removeToast(t.id)}
                >
                    <span
                        className="toast-icon"
                        dangerouslySetInnerHTML={{ __html: ICONS[t.type] }}
                    />
                    <span className="toast-message">{t.message}</span>
                    <button type="button" aria-label="Fechar notificação" className="toast-close" onClick={() => removeToast(t.id)}>✕</button>
                </div>
            ))}
        </div>
    );
}
