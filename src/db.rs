use anyhow::anyhow;
use anyhow::Result;
use directories::ProjectDirs;
use rusqlite::Connection;
use tokio::sync::mpsc::Sender;
use tokio::sync::{mpsc::Receiver, oneshot};

use crate::server::commands::do_advertise;
use crate::server::commands::do_save_peer;
use crate::server::commands::AdvertiseMessage;
use crate::{
    cli::parse::Arg,
    ipc::{Job, Task},
    server::commands::{do_complete, do_dispatch, do_submit, DispatchedJob},
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
    },
    Advertise,
    SavePeer {
        message: AdvertiseMessage,
    },
}

pub async fn loop_db(
    mut rx: Receiver<SQLiteCommand>,
    tx: Sender<Option<AdvertiseMessage>>,
) -> Result<()> {
    let mut conn = init()?;
    println!("{:?}", tx);

    loop {
        let cmd = rx
            .recv()
            .await
            .ok_or_else(|| anyhow!("Failed to receive command"))?;
        match cmd {
            SQLiteCommand::SaveTask { task, jobs, reply } => {
                if reply.send(do_submit(&mut conn, task, jobs)).is_err() {
                    eprintln!("Failed to send reply to submit command");
                }
            }
            SQLiteCommand::Dispatch { reply } => {
                if reply.send(do_dispatch(&mut conn)).is_err() {
                    eprintln!("Failed to send reply to dispatch command");
                }
            }
            SQLiteCommand::Complete { job, success } => {
                do_complete(&mut conn, job, success)?;
            }
            SQLiteCommand::Advertise => {
                if tx.send(do_advertise(&mut conn)?).await.is_err() {
                    eprintln!("Failed to send advertise message");
                }
            }
            SQLiteCommand::SavePeer { message } => {
                if do_save_peer(&mut conn, &message).is_err() {
                    eprintln!("Failed to save peer");
                }
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
