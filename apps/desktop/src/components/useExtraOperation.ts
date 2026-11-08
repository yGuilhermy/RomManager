import { useEffect, useRef, useState } from "react";
import { apiJson, apiFetch, ApiError, invalidatePcsx2Cache } from "../lib/api";
import { saveGeneratedFile } from "../lib/nativeFiles";
import type { JobItem } from "../types";
import type { ExtraOperation, ExtraRequest } from "../lib/extras";
type Stored = {
  id: string;
  action: "download" | "install";
  prompted?: boolean;
};
const KEY = "rommanager-extra-operation";
function stored(): Stored | null {
  try {
    const value = JSON.parse(sessionStorage.getItem(KEY) || "null");
    return /^job_[\d_]+$/.test(value?.id) &&
      ["download", "install"].includes(value?.action)
      ? value
      : null;
  } catch {
    return null;
  }
}
export function useExtraOperation() {
  const [task, setTask] = useState<Stored | null>(stored);
  const [operation, setOperation] = useState<ExtraOperation | null>(null);
  const [starting, setStarting] = useState(false),
    [saving, setSaving] = useState(false),
    [error, setError] = useState(""),
    [notice, setNotice] = useState("");
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  const remember = (value: Stored) => {
    sessionStorage.setItem(KEY, JSON.stringify(value));
    setTask(value);
  };
  useEffect(() => {
    if (!task) return;
    let active = true;
    let timer: ReturnType<typeof setTimeout>;
    const poll = async () => {
      try {
        const value = await apiJson<ExtraOperation>(
          `/api/extras/operations/${encodeURIComponent(task.id)}`,
        );
        if (!active) return;
        setOperation(value);
        if (
          ["running", "queued", "cancel_requested"].includes(value.job.status)
        ) {
          timer = setTimeout(poll, 1000);
        } else if (value.job.status === "succeeded" && value.result?.applied) {
          invalidatePcsx2Cache("/api/pcsx2/resources");
          invalidatePcsx2Cache("/api/pcsx2/memory-cards");
        }
      } catch (e) {
        if (active) {
          if(e instanceof ApiError&&e.status===404){sessionStorage.removeItem(KEY);setTask(null);setOperation(null);setError("A tarefa foi removida. Você pode iniciar outro download.");return;}
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
  }, [task?.id]);
  const save = async () => {
    if (!operation?.result || saving) return;
    setSaving(true);
    setError("");
    try {
      const path = await saveGeneratedFile({
        kind: "extra",
        source: operation.result.path,
      });
      if (mounted.current)
        setNotice(
          path
            ? `Arquivo salvo em ${path}`
            : "Download concluído. Use Salvar arquivo quando quiser escolher o destino.",
        );
    } catch (e) {
      if (mounted.current) setError(e instanceof Error ? e.message : String(e));
    } finally {
      if (mounted.current) setSaving(false);
    }
  };
  useEffect(() => {
    if (
      task?.action === "download" &&
      !task.prompted &&
      operation?.job.id === task.id &&
      operation.job.status === "succeeded" &&
      operation.result
    ) {
      remember({ ...task, prompted: true });
      void save();
    }
  }, [task?.id, task?.prompted, operation?.job.status]);
  const start = async (request: ExtraRequest) => {
    setStarting(true);
    setError("");
    setNotice("");
    try {
      const job = await apiJson<JobItem>("/api/extras/operations", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(request),
      });
      setOperation({ job, result: null });
      remember({ id: job.id, action: request.action });
      return true;
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      return false;
    } finally {
      setStarting(false);
    }
  };
  const cancel = async () => {
    if (!task) return;
    try {
      await apiFetch(`/api/jobs/${encodeURIComponent(task.id)}/cancel`, {
        method: "POST",
      });
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };
  return {
    operation,
    starting,
    saving,
    error,
    notice,
    start,
    save,
    cancel,
    dismiss: () => {
      sessionStorage.removeItem(KEY);
      setTask(null);setOperation(null);setNotice("");setError("");
    },
    busy:
      starting ||
      (!!operation &&
        ["running", "queued", "cancel_requested"].includes(
          operation.job.status,
        )),
  };
}
