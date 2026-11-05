import { useState } from "react";
import type { JobItem } from "../types";
import { apiFetch } from "../lib/api";
import { showToast } from "./Toast";

interface ActivityPageProps {
    jobs: JobItem[];
    onRefresh: () => Promise<void>;
}

const JOB_STATUS_LABEL: Record<string, string> = {
    queued: "Na fila",
    running: "Em execução",
    cancel_requested: "Cancelando...",
    succeeded: "Concluído",
    failed: "Falhou",
    interrupted: "Interrompido"
};

export function ActivityPage({ jobs, onRefresh }: ActivityPageProps) {
    const [isCreatingTestJob, setIsCreatingTestJob] = useState(false);

    const activeJobs = jobs.filter(j => j.status === "running" || j.status === "cancel_requested");
    const queuedJobs = jobs.filter(j => j.status === "queued");
    const historyJobs = jobs.filter(j => j.status === "succeeded" || j.status === "failed" || j.status === "interrupted");

    const handleCancel = async (id: string) => {
        try {
            await apiFetch(`/api/jobs/${encodeURIComponent(id)}/cancel`, { method: "POST" });
            showToast("Cancelamento solicitado.", "info");
            await onRefresh();
        } catch {
            showToast("Não foi possível solicitar cancelamento da tarefa.", "error");
        }
    };

    const handleDelete = async (id: string) => {
        try {
            await apiFetch(`/api/jobs/${encodeURIComponent(id)}`, { method: "DELETE" });
            showToast("Registro removido.", "info");
            await onRefresh();
        } catch {
            showToast("Erro ao remover registro da tarefa.", "error");
        }
    };

    const handleStartTestJob = async () => {
        setIsCreatingTestJob(true);
        try {
            const res = await apiFetch("/api/jobs", {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({
                    kind: "verification_test",
                    title: "Verificação de integridade de arquivos",
                    total_steps: 10
                })
            });
            if (res.ok) {
                showToast("Tarefa de teste iniciada!", "success");
                await onRefresh();
            } else {
                showToast("Falha ao iniciar tarefa de teste.", "error");
            }
        } catch {
            showToast("Erro de comunicação ao iniciar tarefa.", "error");
        } finally {
            setIsCreatingTestJob(false);
        }
    };

    return (
        <div className="activity-container">
            <header className="page-heading downloads-page-header">
                <div>
                    <h2>Central de Atividade</h2>
                    <p>Tarefas persistidas em segundo plano, conversões, cópias e verificações</p>
                </div>
                <div className="panel-toolbar">
                    <button
                        type="button"
                        className="button-secondary"
                        disabled={isCreatingTestJob}
                        onClick={handleStartTestJob}
                    >
                        {isCreatingTestJob ? "Iniciando…" : "+ Nova tarefa de teste"}
                    </button>
                    <button type="button" className="button-quiet" onClick={() => void onRefresh()}>
                        Atualizar
                    </button>
                </div>
            </header>

            <section className="panel">
                <div className="panel-header">
                    <h3>Em execução ({activeJobs.length})</h3>
                </div>
                {!activeJobs.length && (
                    <p className="field-hint">Nenhuma operação ativa no momento.</p>
                )}
                {activeJobs.map(job => (
                    <article className="download-row-v2" key={job.id}>
                        <div className="download-info-v2">
                            <h4>{job.title}</h4>
                            <p className="field-hint">
                                <span className="status-badge running">{JOB_STATUS_LABEL[job.status] || job.status}</span>
                                {job.current_step && ` · ${job.current_step}`}
                                {job.total_steps > 0 && ` (Etapa ${job.completed_steps}/${job.total_steps})`}
                            </p>
                            <progress
                                aria-label={`Progresso de ${job.title}`}
                                max={100}
                                value={job.progress}
                            />
                            <p className="field-hint">{job.progress.toFixed(1)}% concluído</p>
                        </div>
                        {job.status === "running" ? (
                            <button
                                type="button"
                                className="button-quiet danger-text"
                                onClick={() => void handleCancel(job.id)}
                            >
                                Cancelar
                            </button>
                        ) : (
                            <button
                                type="button"
                                className="button-quiet"
                                disabled
                            >
                                Cancelando…
                            </button>
                        )}
                    </article>
                ))}
            </section>

            <section className="panel">
                <div className="panel-header">
                    <h3>Na fila ({queuedJobs.length})</h3>
                </div>
                {!queuedJobs.length && (
                    <p className="field-hint">Nenhuma operação pendente na fila.</p>
                )}
                {queuedJobs.map((job, idx) => (
                    <article className="download-row-v2" key={job.id}>
                        <div className="download-info-v2">
                            <div className="download-queue-title">
                                <span className="queue-position-badge">#{idx + 1}</span>
                                <h4>{job.title}</h4>
                            </div>
                            <p className="field-hint">Aguardando execução · {job.kind}</p>
                        </div>
                        {job.status === "queued" ? (
                            <button
                                type="button"
                                className="button-quiet danger-text"
                                onClick={() => void handleCancel(job.id)}
                            >
                                Cancelar
                            </button>
                        ) : (
                            <button
                                type="button"
                                className="button-quiet"
                                disabled
                            >
                                Cancelando…
                            </button>
                        )}
                    </article>
                ))}
            </section>

            <section className="panel download-history-v2">
                <div className="panel-header">
                    <h3>Histórico de operações ({historyJobs.length})</h3>
                </div>
                {!historyJobs.length && (
                    <p className="field-hint">Nenhuma operação finalizada registrada.</p>
                )}
                {historyJobs.map(job => (
                    <article className="download-row-v2" key={job.id}>
                        <div className="download-info-v2">
                            <h4>{job.title}</h4>
                            <p className="field-hint">
                                <span className={`status-badge ${job.status}`}>
                                    {JOB_STATUS_LABEL[job.status] || job.status}
                                </span>
                                {job.finished_at && ` · Concluído em ${new Date(job.finished_at).toLocaleTimeString()}`}
                            </p>
                            {job.error_message && (
                                <p className="download-error">{job.error_message}</p>
                            )}
                        </div>
                        <div className="panel-toolbar">
                            <button
                                type="button"
                                className="button-quiet"
                                onClick={() => void handleDelete(job.id)}
                            >
                                Remover
                            </button>
                        </div>
                    </article>
                ))}
            </section>
        </div>
    );
}
