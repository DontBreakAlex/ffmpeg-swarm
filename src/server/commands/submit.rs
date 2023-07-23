use anyhow::{Error, Result};
use diesel::{Connection, SqliteConnection};
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

pub fn do_submit(conn: &mut SqliteConnection, task: Task, jobs: Vec<Job>) -> Result<u32> {
    conn.transaction(|conn| {
        let task_id = save_task(conn, &task)?;
        save_jobs(conn, task_id, &jobs)?;
        Ok(task_id as u32)
    })
}

fn save_task(conn: &mut SqliteConnection, task: &Task) -> Result<i64> {
    use crate::schema::tasks::dsl::*;
    // let mut stmt = conn.prepare_cached(r#"INSERT INTO tasks (name, args) VALUES ("TASK", ?)"#)?;
    // let args = serde_json::to_string(&task.args)?;
    // let id = stmt.insert(params![args])?;
    let id = diesel::insert_into(tasks)
        .values(task)
        .execute(conn)?;

    println!("Task id: {}", id);
    Ok(id)
}

fn save_jobs(conn: &mut SqliteConnection, task_id: i64, jobs: &[Job]) -> Result<()> {
    let mut stmt =
        conn.prepare_cached(r#"INSERT INTO jobs (task_id, inputs, output) VALUES (?, ?, ?)"#)?;
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
