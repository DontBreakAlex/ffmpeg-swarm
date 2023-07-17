use std::path::PathBuf;

use crate::{cli::parse::Arg, db::SQLiteCommand, ipc::Job};
use anyhow::Result;
use rusqlite::Connection;
use tokio::sync::mpsc::Sender;

pub struct DispatchedJob {
    pub job_id: u32,
    pub task_id: u32,
    pub job: Job,
    pub args: Vec<Arg>,
}

pub async fn handle_dispatch(tx: Sender<SQLiteCommand>) -> Result<DispatchedJob> {
    let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();

    tx.send(SQLiteCommand::Dispatch { reply: reply_tx }).await?;

    let job = reply_rx.await??;

    Ok(job)
}

pub fn do_dispatch(conn: &mut Connection) -> Result<DispatchedJob> {
    let tx = conn.transaction()?;
    let mut get_job = tx.prepare_cached("SELECT jobs.id, task_id, inputs, output, args FROM jobs INNER JOIN tasks t on t.id = jobs.task_id WHERE jobs.started_at IS NULL ORDER BY jobs.created_at LIMIT 1;")?;
    let mut start_task = tx.prepare_cached(
        "UPDATE tasks SET started_at = coalesce(started_at, CURRENT_TIMESTAMP) WHERE id = ?;",
    )?;
    let mut start_job =
        tx.prepare_cached("UPDATE jobs SET started_at = CURRENT_TIMESTAMP WHERE id = ?;")?;

    let (job_id, task_id, job, args) = get_job.query_row([], |row| {
        let job_id: u32 = row.get(0)?;
        let task_id: u32 = row.get(1)?;
        let inputs: String = row.get(2)?;
        let output: String = row.get(3)?;

        let job = Job {
            inputs: serde_json::from_str(&inputs)
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
            output: PathBuf::from(output),
        };

        let args: Vec<Arg> = serde_json::from_str(&row.get::<_, String>(4)?)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;

        Ok((job_id, task_id, job, args))
    })?;

    start_task.execute([task_id])?;
    start_job.execute([job_id])?;

    drop((get_job, start_task, start_job));

    tx.commit()?;
    Ok(DispatchedJob {
        job_id,
        task_id,
        job,
        args,
    })
}
