import { useState, useEffect } from "react";

const resolved = new Map<string, string | null>();
const MAX_RESOLVED = 1500;
const STORAGE_KEY = "rom_cover_resolution";

function loadStored() {
    try {
        const raw = sessionStorage.getItem(STORAGE_KEY);
        if (!raw) return;
        for (const [key, value] of JSON.parse(raw) as [string, string | null][]) resolved.set(key, value);
    } catch {}
}
loadStored();

let persistTimer: ReturnType<typeof setTimeout> | undefined;
function remember(key: string, value: string | null) {
    resolved.delete(key);
    resolved.set(key, value);
    while (resolved.size > MAX_RESOLVED) {
        const oldest = resolved.keys().next().value;
        if (oldest === undefined) break;
        resolved.delete(oldest);
    }
    clearTimeout(persistTimer);
    persistTimer = setTimeout(() => {
        try { sessionStorage.setItem(STORAGE_KEY, JSON.stringify([...resolved.entries()])); } catch {}
    }, 500);
}

const keyOf = (src: string | null | undefined, title: string) => `${src || ""}\u0000${title}`;

function initial(src: string | null | undefined, title: string) {
    const known = resolved.get(keyOf(src, title));
    if (known === null) return { failed: true, current: "" };
    if (known) return { failed: false, current: known };
    return { failed: !src, current: src || "" };
}

export function CoverImage({ src, title, region, className, style }: { src?: string | null; title: string; region?: string; className?: string; style?: React.CSSProperties }) {
    const [failed, setFailed] = useState(() => initial(src, title).failed);
    const [attempt, setAttempt] = useState(0);
    const [currentSrc, setCurrentSrc] = useState(() => initial(src, title).current);

    useEffect(() => {
        const next = initial(src, title);
        setFailed(next.failed);
        setAttempt(0);
        setCurrentSrc(next.current);
    }, [src, title]);

    const handleLoad = () => {
        if (currentSrc) remember(keyOf(src, title), currentSrc);
    };

    const handleError = () => {
        if (attempt === 0) {
            setAttempt(1);
            let alt = title.replace(/\(USA,\s*Canada\)/g, "(USA)").replace(/[&:/\\*?"<>|]/g, "_").trim();
            if (alt !== title) {
                setCurrentSrc(`https://cdn.jsdelivr.net/gh/libretro-thumbnails/Sony_-_PlayStation_2@master/Named_Boxarts/${encodeURIComponent(alt)}.png`);
                return;
            }
        }
        if (attempt <= 1) {
            setAttempt(2);
            let base = title.replace(/\s*\([^)]*\)$/, "").replace(/[&:/\\*?"<>|]/g, "_").trim();
            if (base && base !== title) {
                setCurrentSrc(`https://cdn.jsdelivr.net/gh/libretro-thumbnails/Sony_-_PlayStation_2@master/Named_Boxarts/${encodeURIComponent(base)}%20(USA).png`);
                return;
            }
        }
        if (attempt <= 2) {
            setAttempt(3);
            let raw = title.replace(/[&:/\\*?"<>|]/g, "_").trim();
            setCurrentSrc(`https://raw.githubusercontent.com/libretro-thumbnails/Sony_-_PlayStation_2/master/Named_Boxarts/${encodeURIComponent(raw)}.png`);
            return;
        }
        remember(keyOf(src, title), null);
        setFailed(true);
    };

    if (failed || !currentSrc) {
        return (
            <div className={`ps2-fallback-cover ${className || ""}`} style={style}>
                <div className="ps2-fallback-spine" />
                <div className="ps2-fallback-content">
                    <div className="ps2-fallback-header">
                        <span className="ps2-logo-text">PlayStation 2</span>
                    </div>
                    <div className="ps2-fallback-disc">
                        <div className="ps2-disc-ring" />
                    </div>
                    <div className="ps2-fallback-title">{title}</div>
                    <div className="ps2-fallback-footer">
                        <span className="ps2-badge">PAL/NTSC</span>
                        <span className="ps2-badge">{region || "DVD-ROM"}</span>
                    </div>
                </div>
            </div>
        );
    }

    return (
        <img
            src={currentSrc}
            alt={title}
            loading="lazy"
            decoding="async"
            onLoad={handleLoad}
            onError={handleError}
            className={className}
            style={style}
        />
    );
}
