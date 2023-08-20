use std::path::PathBuf;

use crate::{cli::parse::Arg, db::SQLiteCommand, ipc::Job};
use anyhow::Result;
use rusqlite::{Connection, OptionalExtension, Row};
use tokio::sync::mpsc::Sender;

pub struct LocalJob {
    pub job_id: u32,
    pub task_id: u32,
    pub job: Job,
    pub args: Vec<Arg>,
}

impl LocalJob {
    pub fn from_row(r: &Row) -> Result<Self, rusqlite::Error> {
        Ok(Self {
            job_id: r.get(0)?,
            task_id: r.get(1)?,
            job: Job {
                inputs: serde_json::from_str(&r.get::<_, String>(2)?)
                    .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
                output: PathBuf::from(r.get::<_, String>(3)?),
            },
            args: serde_json::from_str(&r.get::<_, String>(4)?)
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
        })
    }
}

pub async fn handle_dispatch(tx: &Sender<SQLiteCommand>) -> Result<Option<LocalJob>> {
    let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();

    tx.send(SQLiteCommand::Dispatch { reply: reply_tx }).await?;

    let job = reply_rx.await??;

    Ok(job)
}

pub fn do_dispatch(conn: &mut Connection) -> Result<Option<LocalJob>> {
    let tx = conn.transaction()?;
    let mut get_job = tx.prepare_cached("SELECT jobs.id, task_id, inputs, output, args FROM jobs INNER JOIN tasks t on t.id = jobs.task_id WHERE jobs.started_at IS NULL ORDER BY jobs.created_at LIMIT 1;")?;
    let mut start_task = tx.prepare_cached(
        "UPDATE tasks SET started_at = coalesce(started_at, CURRENT_TIMESTAMP) WHERE id = ?;",
    )?;
    let mut start_job =
        tx.prepare_cached("UPDATE jobs SET started_at = CURRENT_TIMESTAMP WHERE id = ?;")?;

    let job = get_job.query_row([], LocalJob::from_row).optional()?;

    if let Some(job) = &job {
        start_task.execute([job.task_id])?;
        start_job.execute([job.job_id])?;
    }

    drop((get_job, start_task, start_job));

    tx.commit()?;
    Ok(job)
}
