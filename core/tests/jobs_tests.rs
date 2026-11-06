use rom_core::database;
use rom_core::jobs::JobManager;
use std::fs;
use std::path::Path;
use std::time::Duration;

#[tokio::test]
async fn job_lifecycle_runs_and_succeeds() {
    let test_dir = Path::new("target/test_jobs_lifecycle");
    if test_dir.exists() {
        let _ = fs::remove_dir_all(test_dir);
    }
    fs::create_dir_all(test_dir).expect("create test dir");
    let db_path = test_dir.join("test_lifecycle.db");

    let _conn = database::initialize_db(&db_path).expect("init db");
    let mgr = JobManager::new(db_path);

    let job = mgr
        .create_job("test_op", "Test Operation", 3)
        .expect("create job");
    assert_eq!(job.status, "running");
    assert_eq!(job.total_steps, 3);

    let polled_initial = mgr.get_job(&job.id).expect("get job").expect("found");
    assert_eq!(polled_initial.id, job.id);

    for _ in 0..25 {
        tokio::time::sleep(Duration::from_millis(200)).await;
        if let Some(j) = mgr.get_job(&job.id).expect("get job") {
            if j.status == "succeeded" {
                assert_eq!(j.progress, 100.0);
                assert!(j.finished_at.is_some());
                assert_eq!(j.completed_steps, 3);
                return;
            }
        }
    }

    panic!("Job did not succeed in time");
}

#[tokio::test]
async fn job_cancellation_stops_worker_and_marks_failed() {
    let test_dir = Path::new("target/test_jobs_cancel");
    if test_dir.exists() {
        let _ = fs::remove_dir_all(test_dir);
    }
    fs::create_dir_all(test_dir).expect("create test dir");
    let db_path = test_dir.join("test_cancel.db");

    let _conn = database::initialize_db(&db_path).expect("init db");
    let mgr = JobManager::new(db_path);

    let job = mgr
        .create_job("long_op", "Long Running Operation", 30)
        .expect("create job");

    let cancel_res = mgr.request_cancel(&job.id).expect("request cancel");
    assert!(cancel_res);

    for _ in 0..20 {
        tokio::time::sleep(Duration::from_millis(150)).await;
        if let Some(j) = mgr.get_job(&job.id).expect("get job") {
            if j.status == "failed" {
                assert!(j.error_message.unwrap().contains("Cancelamento confirmado"));
                return;
            }
        }
    }

    panic!("Job was not cancelled in time");
}

#[tokio::test]
async fn restart_recovers_orphaned_jobs_as_interrupted() {
    let test_dir = Path::new("target/test_jobs_restart");
    if test_dir.exists() {
        let _ = fs::remove_dir_all(test_dir);
    }
    fs::create_dir_all(test_dir).expect("create test dir");
    let db_path = test_dir.join("test_restart.db");

    {
        let conn = database::initialize_db(&db_path).expect("init db");
        conn.execute(
            "INSERT INTO jobs (id, kind, title, status, progress, current_step, total_steps, completed_steps, created_at, updated_at)
             VALUES ('job_crashed', 'copy', 'Unfinished Copy', 'running', 45.0, 'Copying part 2', 5, 2, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        ).expect("insert crashed job");
    }

    let mgr = JobManager::new(db_path);
    let job = mgr.get_job("job_crashed").expect("get job").expect("found");
    assert_eq!(job.status, "interrupted");
    assert!(job.error_message.unwrap().contains("interrompida"));
    assert!(job.finished_at.is_some());
}

#[tokio::test]
async fn queued_job_cancellation_immediately_marks_failed() {
    let test_dir = Path::new("target/test_jobs_queued_cancel");
    if test_dir.exists() {
        let _ = fs::remove_dir_all(test_dir);
    }
    fs::create_dir_all(test_dir).expect("create test dir");
    let db_path = test_dir.join("test_queued_cancel.db");

    let _init = database::initialize_db(&db_path).expect("init db");
    let mgr = JobManager::new(db_path.clone());

    {
        let conn = rusqlite::Connection::open(&db_path).expect("open db");
        conn.execute(
            "INSERT INTO jobs (id, kind, title, status, progress, current_step, total_steps, completed_steps, created_at, updated_at)
             VALUES ('job_queued_1', 'chd', 'Pending Conversion', 'queued', 0.0, 'Waiting', 10, 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        ).expect("insert queued job");
    }

    let res = mgr.request_cancel("job_queued_1").expect("cancel queued");
    assert!(res);

    let job = mgr.get_job("job_queued_1").expect("get job").expect("found");
    assert_eq!(job.status, "failed");
    assert!(job.finished_at.is_some());
    assert!(job.error_message.unwrap().contains("Cancelamento"));
}

#[tokio::test]
async fn delete_job_aborts_active_runner_and_removes_record() {
    let test_dir = Path::new("target/test_jobs_delete");
    if test_dir.exists() {
        let _ = fs::remove_dir_all(test_dir);
    }
    fs::create_dir_all(test_dir).expect("create test dir");
    let db_path = test_dir.join("test_delete.db");

    let _conn = database::initialize_db(&db_path).expect("init db");
    let mgr = JobManager::new(db_path);

    let job = mgr.create_job("temp", "Temporary Task", 20).expect("create job");
    assert_eq!(job.status, "running");

    let del = mgr.delete_job(&job.id).expect("delete job");
    assert!(del);

    let fetched = mgr.get_job(&job.id).expect("get job");
    assert!(fetched.is_none());
}

#[tokio::test]
async fn status_polling_does_not_mutate_state() {
    let test_dir = Path::new("target/test_jobs_readonly");
    if test_dir.exists() {
        let _ = fs::remove_dir_all(test_dir);
    }
    fs::create_dir_all(test_dir).expect("create test dir");
    let db_path = test_dir.join("test_readonly.db");

    {
        let conn = database::initialize_db(&db_path).expect("init db");
        conn.execute(
            "INSERT INTO jobs (id, kind, title, status, progress, current_step, total_steps, completed_steps, created_at, updated_at)
             VALUES ('job_static', 'check', 'Static Task', 'running', 20.0, 'Step 2', 10, 2, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        ).expect("insert static job");
    }

    let mgr = JobManager::new(db_path);
    let first = mgr.get_job("job_static").expect("get 1").expect("found");
    let second = mgr.get_job("job_static").expect("get 2").expect("found");

    assert_eq!(first.status, second.status);
    assert_eq!(first.progress, second.progress);
    assert_eq!(first.updated_at, second.updated_at);
}
