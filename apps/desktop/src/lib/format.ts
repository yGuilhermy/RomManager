export function formatBytes(bytes: number): string {
    if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
    const units = ["B", "KB", "MB", "GB", "TB", "PB"];
    const index = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
    return `${parseFloat((bytes / 1024 ** index).toFixed(2))} ${units[index]}`;
}
export function formatDate(value: string | null): string {
    if (!value) return "Ainda não iniciado";
    const date = new Date(value);
    return Number.isNaN(date.getTime()) ? "Data indisponível" : date.toLocaleDateString("pt-BR", {day:"2-digit",month:"short",year:"numeric"});
}
