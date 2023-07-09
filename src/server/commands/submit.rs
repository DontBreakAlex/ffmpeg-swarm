use anyhow::{Error, Result};
use rusqlite::{params, Connection, Transaction};
use tokio::sync::mpsc::Sender;

use crate::{
    db::SQLiteCommand,
    ipc::{Job, ServiceToCli, Task},
};

pub async fn handle_submit(
    tx: Sender<SQLiteCommand>,
    task: Task,
    jobs: Vec<Job>,
) -> Result<ServiceToCli> {
    let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
    tx.send(SQLiteCommand::SaveTask {
        task,
        jobs,
        reply: reply_tx,
    })
    .await?;
    let id = reply_rx.await??;
    Ok(ServiceToCli::TaskCreated { id })
}

pub fn do_submit(conn: &mut Connection, task: Task, jobs: Vec<Job>) -> Result<u32> {
    let tx = conn.transaction()?;
    let task_id = save_task(&tx, &task)?;
    save_jobs(&tx, task_id, &jobs)?;
    tx.commit()?;
    Ok(task_id as u32)
}

fn save_task(tx: &Transaction, task: &Task) -> Result<i64> {
    let mut stmt = tx.prepare_cached(r#"INSERT INTO tasks (name, args) VALUES ("TASK", ?)"#)?;
    let args = serde_json::to_string(&task.args)?;
    let id = stmt.insert(params![args])?;
    println!("Task id: {}", id);
    Ok(id)
}

fn save_jobs(tx: &Transaction, task_id: i64, jobs: &[Job]) -> Result<()> {
    let mut stmt =
        tx.prepare_cached(r#"INSERT INTO jobs (task_id, inputs, output) VALUES (?, ?, ?)"#)?;
    for job in jobs {
        let inputs = serde_json::to_string(&job.inputs)?;
        let output = job
            .output
            .to_str()
            .ok_or(Error::msg("Failed to serialize output path"))?;
        stmt.execute(params![task_id, inputs, output])?;
    }
    Ok(())
}
