use anyhow::Result;
use directories::ProjectDirs;
use rusqlite::Connection;
use tokio::sync::{mpsc::Receiver, oneshot};
use anyhow::anyhow;

use crate::{
    ipc::{Job, Task},
    server::commands::{do_submit, do_dispatch, DispatchedJob, do_complete}, cli::parse::Arg,
};

pub enum SQLiteCommand {
    SaveTask {
        task: Task,
        jobs: Vec<Job>,
        reply: oneshot::Sender<Result<u32>>,
    },
    Dispatch {
        reply: oneshot::Sender<Result<DispatchedJob>>,
    },
    Complete {
        job: u32,
        success: bool,
    }
}

pub async fn loop_db(mut rx: Receiver<SQLiteCommand>) -> Result<()> {
    let mut conn = init()?;

    loop {
        let cmd = rx.recv().await.ok_or_else(|| anyhow!("Failed to receive command"))?;
        match cmd {
            SQLiteCommand::SaveTask { task, jobs, reply } => {
                if reply.send(do_submit(&mut conn, task, jobs)).is_err() {
                    eprintln!("Failed to send reply to submit command");
                }
            },
            SQLiteCommand::Dispatch { reply } => {
                if reply.send(do_dispatch(&mut conn)).is_err() {
                    eprintln!("Failed to send reply to dispatch command");
                }
            },
            SQLiteCommand::Complete { job, success } => {
                do_complete(&mut conn, job, success)?;
            }
        }
    }
}

pub fn init() -> Result<Connection> {
    let dirs = ProjectDirs::from("none", "dontbreakalex", "ffmpeg-swarm").unwrap();
    let path = dirs.data_dir();
    std::fs::create_dir_all(&path)?;
    let db_path = path.join("ffmpeg-swarm.db");
    let conn = Connection::open(db_path)?;
    conn.execute_batch(include_str!("./init.sql"))?;

    Ok(conn)
}
