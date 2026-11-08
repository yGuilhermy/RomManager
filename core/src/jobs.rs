use chrono::Utc;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

static JOB_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobRecord {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub status: String,
    pub progress: f64,
    pub current_step: String,
    pub total_steps: i64,
    pub completed_steps: i64,
    pub error_message: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub finished_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateJobPayload {
    pub kind: String,
    pub title: String,
    pub total_steps: Option<i64>,
}

#[derive(Clone)]
pub struct JobManager {
    db_path: PathBuf,
    cancellations: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
}

impl JobManager {
    pub fn new(db_path: PathBuf) -> Self {
        let mgr = Self {
            db_path,
            cancellations: Arc::new(Mutex::new(HashMap::new())),
        };
        let _ = mgr.recover_interrupted();
        mgr
    }

    pub fn recover_interrupted(&self) -> rusqlite::Result<usize> {
        let conn = Connection::open(&self.db_path)?;
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE jobs
             SET status = 'interrupted',
                 error_message = 'Operação interrompida antes da conclusão',
                 updated_at = ?1,
                 finished_at = ?1
             WHERE status IN ('running', 'cancel_requested', 'queued')",
            params![now],
        )
    }

    pub fn list_jobs(&self) -> rusqlite::Result<Vec<JobRecord>> {
        let conn = Connection::open(&self.db_path)?;
        let mut stmt = conn.prepare(
            "SELECT id, kind, title, status, progress, current_step, total_steps, completed_steps, error_message, created_at, updated_at, finished_at
             FROM jobs
             ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(JobRecord {
                id: r.get(0)?,
                kind: r.get(1)?,
                title: r.get(2)?,
                status: r.get(3)?,
                progress: r.get(4)?,
                current_step: r.get(5)?,
                total_steps: r.get(6)?,
                completed_steps: r.get(7)?,
                error_message: r.get(8)?,
                created_at: r.get(9)?,
                updated_at: r.get(10)?,
                finished_at: r.get(11)?,
            })
        })?;
        let mut list = Vec::new();
        for item in rows.flatten() {
            list.push(item);
        }
        Ok(list)
    }

    pub fn get_job(&self, id: &str) -> rusqlite::Result<Option<JobRecord>> {
        let conn = Connection::open(&self.db_path)?;
        let mut stmt = conn.prepare(
            "SELECT id, kind, title, status, progress, current_step, total_steps, completed_steps, error_message, created_at, updated_at, finished_at
             FROM jobs
             WHERE id = ?1",
        )?;
        let mut rows = stmt.query(params![id])?;
        if let Some(r) = rows.next()? {
            Ok(Some(JobRecord {
                id: r.get(0)?,
                kind: r.get(1)?,
                title: r.get(2)?,
                status: r.get(3)?,
                progress: r.get(4)?,
                current_step: r.get(5)?,
                total_steps: r.get(6)?,
                completed_steps: r.get(7)?,
                error_message: r.get(8)?,
                created_at: r.get(9)?,
                updated_at: r.get(10)?,
                finished_at: r.get(11)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn begin_task(&self, kind: &str, title: &str, total_steps: i64) -> rusqlite::Result<JobRecord> {
        let seq = JOB_COUNTER.fetch_add(1, Ordering::Relaxed);
        let id = format!("job_{}_{}", Utc::now().timestamp_millis(), seq);
        let now = Utc::now().to_rfc3339();
        let total = if total_steps <= 0 { 1 } else { total_steps };

        let record = JobRecord {
            id: id.clone(),
            kind: kind.to_string(),
            title: title.to_string(),
            status: "running".to_string(),
            progress: 0.0,
            current_step: "Iniciando processamento".to_string(),
            total_steps: total,
            completed_steps: 0,
            error_message: None,
            created_at: now.clone(),
            updated_at: now,
            finished_at: None,
        };

        {
            let conn = Connection::open(&self.db_path)?;
            conn.execute(
                "INSERT INTO jobs (id, kind, title, status, progress, current_step, total_steps, completed_steps, error_message, created_at, updated_at, finished_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    record.id,
                    record.kind,
                    record.title,
                    record.status,
                    record.progress,
                    record.current_step,
                    record.total_steps,
                    record.completed_steps,
                    record.error_message,
                    record.created_at,
                    record.updated_at,
                    record.finished_at,
                ],
            )?;
        }

        let cancel_flag = Arc::new(AtomicBool::new(false));
        self.cancellations
            .lock()
            .unwrap()
            .insert(id.clone(), Arc::clone(&cancel_flag));

        Ok(record)
    }

    pub fn create_job(&self, kind: &str, title: &str, total_steps: i64) -> rusqlite::Result<JobRecord> {
        let record = self.begin_task(kind, title, total_steps)?;
        let id = record.id.clone();
        let total = record.total_steps;
        let cancel_flag = self.cancellations.lock().unwrap().get(&id).cloned().unwrap();
        let runner = self.clone();
        tokio::spawn(async move {
            runner.run_simulated_job(id, total, cancel_flag).await;
        });

        Ok(record)
    }

    pub fn cancelled(&self, id: &str) -> bool {
        self.cancellations.lock().map(|m| m.get(id).is_none_or(|v| v.load(Ordering::Relaxed))).unwrap_or(true)
    }

    pub fn task_progress(&self, id: &str, percent: f64, message: &str) -> rusqlite::Result<()> {
        Connection::open(&self.db_path)?.execute(
            "UPDATE jobs SET progress=?1,completed_steps=CAST(total_steps*?1/100 AS INTEGER),current_step=?2,updated_at=?3 WHERE id=?4 AND status IN ('running','cancel_requested')",
            params![percent.clamp(0.0, 100.0),message,Utc::now().to_rfc3339(),id])?;
        Ok(())
    }

    pub fn finish_task(&self, id: &str, result: Result<&str, &str>) -> rusqlite::Result<()> {
        let (status, message, error) = match result {
            Ok(message) => ("succeeded", message, None),
            Err(error) => ("failed", "Operação não concluída", Some(error)),
        };
        Connection::open(&self.db_path)?.execute(
            "UPDATE jobs SET status=?1,current_step=?2,error_message=?3,progress=CASE WHEN ?1='succeeded' THEN 100 ELSE progress END,completed_steps=CASE WHEN ?1='succeeded' THEN total_steps ELSE completed_steps END,updated_at=?4,finished_at=?4 WHERE id=?5",
            params![status,message,error,Utc::now().to_rfc3339(),id])?;
        self.cancellations.lock().unwrap().remove(id);
        Ok(())
    }

    pub fn request_cancel(&self, id: &str) -> rusqlite::Result<bool> {
        let conn = Connection::open(&self.db_path)?;
        let status: Option<String> = conn
            .query_row("SELECT status FROM jobs WHERE id = ?1", params![id], |r| r.get(0))
            .ok();

        if let Some(s) = status {
            if s == "queued" {
                if let Some(flag) = self.cancellations.lock().unwrap().remove(id) {
                    flag.store(true, Ordering::Relaxed);
                }
                let now = Utc::now().to_rfc3339();
                conn.execute(
                    "UPDATE jobs SET status = 'failed', error_message = 'Cancelamento confirmado pelo usuário', updated_at = ?1, finished_at = ?1 WHERE id = ?2",
                    params![now, id],
                )?;
                return Ok(true);
            }
            if s == "running" {
                if let Some(flag) = self.cancellations.lock().unwrap().get(id) {
                    flag.store(true, Ordering::Relaxed);
                }
                let now = Utc::now().to_rfc3339();
                conn.execute(
                    "UPDATE jobs SET status = 'cancel_requested', updated_at = ?1 WHERE id = ?2",
                    params![now, id],
                )?;
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub fn delete_job(&self, id: &str) -> rusqlite::Result<bool> {
        if let Some(flag) = self.cancellations.lock().unwrap().remove(id) {
            flag.store(true, Ordering::Relaxed);
        }
        let conn = Connection::open(&self.db_path)?;
        let affected = conn.execute("DELETE FROM jobs WHERE id = ?1", params![id])?;
        Ok(affected > 0)
    }

    async fn run_simulated_job(&self, id: String, total_steps: i64, cancel_flag: Arc<AtomicBool>) {
        let total = total_steps.max(1);
        for step in 1..=total {
            tokio::time::sleep(Duration::from_millis(300)).await;

            if cancel_flag.load(Ordering::Relaxed) {
                let now = Utc::now().to_rfc3339();
                if let Ok(conn) = Connection::open(&self.db_path) {
                    let _ = conn.execute(
                        "UPDATE jobs
                         SET status = 'failed',
                             error_message = 'Cancelamento confirmado pelo usuário',
                             updated_at = ?1,
                             finished_at = ?1
                         WHERE id = ?2",
                        params![now, id],
                    );
                }
                self.cancellations.lock().unwrap().remove(&id);
                return;
            }

            let progress = (step as f64 / total as f64) * 100.0;
            let step_desc = format!("Processando etapa {} de {}", step, total);
            let now = Utc::now().to_rfc3339();

            if let Ok(conn) = Connection::open(&self.db_path) {
                let _ = conn.execute(
                    "UPDATE jobs
                     SET progress = ?1,
                         current_step = ?2,
                         completed_steps = ?3,
                         updated_at = ?4
                     WHERE id = ?5 AND status = 'running'",
                    params![progress, step_desc, step, now, id],
                );
            }
        }

        if cancel_flag.load(Ordering::Relaxed) {
            let now = Utc::now().to_rfc3339();
            if let Ok(conn) = Connection::open(&self.db_path) {
                let _ = conn.execute(
                    "UPDATE jobs
                     SET status = 'failed',
                         error_message = 'Cancelamento confirmado pelo usuário',
                         updated_at = ?1,
                         finished_at = ?1
                     WHERE id = ?2",
                    params![now, id],
                );
            }
            self.cancellations.lock().unwrap().remove(&id);
            return;
        }

        let now = Utc::now().to_rfc3339();
        if let Ok(conn) = Connection::open(&self.db_path) {
            let affected = conn.execute(
                "UPDATE jobs
                 SET status = 'succeeded',
                     progress = 100.0,
                     current_step = 'Concluído com sucesso',
                     completed_steps = total_steps,
                     updated_at = ?1,
                     finished_at = ?1
                 WHERE id = ?2 AND status = 'running'",
                params![now, id],
            ).unwrap_or(0);

            if affected == 0 {
                let status: Option<String> = conn
                    .query_row("SELECT status FROM jobs WHERE id = ?1", params![id], |r| r.get(0))
                    .ok();
                if status.as_deref() == Some("cancel_requested") {
                    let _ = conn.execute(
                        "UPDATE jobs
                         SET status = 'failed',
                             error_message = 'Cancelamento confirmado pelo usuário',
                             updated_at = ?1,
                             finished_at = ?1
                         WHERE id = ?2",
                        params![now, id],
                    );
                }
            }
        }
        self.cancellations.lock().unwrap().remove(&id);
    }
}
