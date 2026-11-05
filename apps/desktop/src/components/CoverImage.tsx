import { useState, useEffect } from "react";

export function CoverImage({ src, title, region, className, style }: { src?: string | null; title: string; region?: string; className?: string; style?: React.CSSProperties }) {
    const [failed, setFailed] = useState(false);
    const [attempt, setAttempt] = useState(0);
    const [currentSrc, setCurrentSrc] = useState(src || "");

    useEffect(() => {
        setFailed(!src);
        setAttempt(0);
        setCurrentSrc(src || "");
    }, [src]);

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
            onError={handleError}
            className={className}
            style={style}
        />
    );
}

