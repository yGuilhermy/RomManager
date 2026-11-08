import { useEffect, useRef, useState } from "react";
import { apiJson,apiFetch,ApiError } from "../lib/api";
import { invalidate } from "../lib/smartCache";
import type { JobItem } from "../types";
type Status = {
  updated_at: string | null;
  total: number;
  source: string;
  schema: number;
};
const KEY = "rommanager-catalog-refresh";
export function CatalogUpdate({ onUpdated }: { onUpdated: () => void }) {
  const [status, setStatus] = useState<Status | null>(null),
    [job, setJob] = useState<JobItem | null>(null),
    [error, setError] = useState("");
  const [id, setId] = useState(() => sessionStorage.getItem(KEY) || ""),
    [starting, setStarting] = useState(false);
  const callback = useRef(onUpdated);
  callback.current = onUpdated;
  useEffect(() => {
    let active = true;
    void apiJson<Status>("/api/catalog/status")
      .then((s) => {
        if (active) setStatus(s);
      })
      .catch(() => {});
    return () => {
      active = false;
    };
  }, []);
  useEffect(() => {
    if (!id) return;
    let active = true;
    let timer: ReturnType<typeof setTimeout>;
    const poll = async () => {
      try {
        const current = await apiJson<JobItem>(
          `/api/jobs/${encodeURIComponent(id)}`,
        );
        if (!active) return;
        setJob(current);
        if (
          ["running", "queued", "cancel_requested"].includes(current.status)
        ) {
          timer = setTimeout(poll, 1000);
          return;
        }
        sessionStorage.removeItem(KEY);
        setId("");
        if (current.status === "succeeded") {
          invalidate("/api/catalog");
          setStatus(await apiJson<Status>("/api/catalog/status"));
          callback.current();
        } else
          setError(
            current.error_message ||
              "Atualização interrompida. O catálogo anterior foi mantido.",
          );
      } catch (e) {
        if (active) {
          if(e instanceof ApiError&&e.status===404){sessionStorage.removeItem(KEY);setId("");setJob(null);setError("O registro da tarefa foi removido. O catálogo atual foi mantido.");return;}
          setError(e instanceof Error ? e.message : String(e));
          timer = setTimeout(poll, 4000);
        }
      }
    };
    void poll();
    return () => {
      active = false;
      clearTimeout(timer);
    };
  }, [id]);
  const update = async () => {
    setStarting(true);
    setError("");
    try {
      const job = await apiJson<JobItem>("/api/catalog/refresh", {
        method: "POST",
      });
      setJob(job);
      sessionStorage.setItem(KEY, job.id);
      setId(job.id);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setStarting(false);
    }
  };
  return (
    <div className="catalog-update">
      <div>
        <p className="field-hint">
          {status?.updated_at
            ? `Dados atualizados em ${new Date(status.updated_at).toLocaleString("pt-BR")}`
            : "Dados incluídos no aplicativo"}{" "}
          · Internet Archive
        </p>
        {id && (
          <p role="status">{job?.current_step || "Consultando atualização…"}</p>
        )}
        {error && (
          <p className="resource-error" role="alert">
            {error}
          </p>
        )}
      </div>
      <div className="extras-actions">
        <button
          type="button"
          className="button-secondary"
          disabled={starting || !!id}
          onClick={() => void update()}
        >
          {starting || id ? "Atualizando catálogo…" : "Atualizar catálogo"}
        </button>
        {id && (
          <button
            type="button"
            className="button-quiet"
            onClick={() =>
              void apiFetch(`/api/jobs/${encodeURIComponent(id)}/cancel`, {
                method: "POST",
              }).catch((e) => setError(String(e)))
            }
          >
            Cancelar atualização
          </button>
        )}
      </div>
    </div>
  );
}
