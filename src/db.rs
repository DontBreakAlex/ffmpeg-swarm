use anyhow::Result;
use rusqlite::Connection;
use tokio::select;
use tokio::sync::mpsc::Sender;
use tokio::sync::{mpsc::Receiver, oneshot};
use tracing::{debug, info};
use uuid::Uuid;

use crate::server::commands::{do_acquire_job, do_advertise, RunnableJob};
use crate::server::commands::{do_remove_peer, do_save_peer};
use crate::server::commands::{do_reset_job, AdvertiseMessage};
use crate::utils::data_dir;
use crate::{
    ipc::{Job, Task},
    server::commands::{do_complete, do_dispatch, do_submit, LocalJob},
};

#[derive(Debug)]
pub enum SQLiteCommand {
    SaveTask {
        task: Task,
        jobs: Vec<Job>,
        reply: oneshot::Sender<Result<u32>>,
    },
    Dispatch {
        reply: oneshot::Sender<Result<Option<LocalJob>>>,
    },
    Complete {
        job_id: u32,
        exit_code: i32,
        completed_by: Uuid,
    },
    Advertise,
    SavePeer {
        message: AdvertiseMessage,
    },
    RemovePeer {
        peer_id: Uuid,
    },
    ResetJob {
        job_id: u32,
    },
}

pub async fn loop_db(
    mut rx: Receiver<SQLiteCommand>,
    advertise_tx: Sender<Option<AdvertiseMessage>>,
    mut request_rx: Receiver<oneshot::Sender<RunnableJob>>,
) -> Result<()> {
    let mut conn = init()?;
    let mut jobs_available = true;

    loop {
        select! {
            biased;
            cmd = rx.recv() => {
                debug!("Db handling command {:?}", cmd);
                handle_cmd(cmd.expect("Database command sender not to be dropped"), &mut conn, &advertise_tx, &mut jobs_available).await?;
            }
            permit = request_rx.recv(), if jobs_available => {
                debug!("Db handling job request");
                if let Some(job) = do_acquire_job(&mut conn)? {
                    permit
                    .expect("Failed to send job to run")
                    .send(job)
                    .expect("Failed to send job to run");
                } else {
                    jobs_available = false;
                }
            }
        }
    }
}

async fn handle_cmd(
    cmd: SQLiteCommand,
    conn: &mut Connection,
    advertise_tx: &Sender<Option<AdvertiseMessage>>,
    jobs_available: &mut bool,
) -> Result<(), anyhow::Error> {
    match cmd {
        SQLiteCommand::SaveTask { task, jobs, reply } => {
            if reply.send(do_submit(conn, task, jobs)).is_err() {
                eprintln!("Failed to send reply to submit command");
            } else {
                *jobs_available = true;
            }
        }
        SQLiteCommand::Dispatch { reply } => {
            if reply.send(do_dispatch(conn)).is_err() {
                eprintln!("Failed to send reply to dispatch command");
            }
        }
        SQLiteCommand::Complete {
            job_id,
            exit_code,
            completed_by,
        } => {
            do_complete(conn, job_id, exit_code, completed_by)?;
        }
        SQLiteCommand::Advertise => {
            if advertise_tx.send(do_advertise(conn)?).await.is_err() {
                eprintln!("Failed to send advertise message");
            }
        }
        SQLiteCommand::SavePeer { message } => {
            if do_save_peer(conn, &message).is_err() {
                eprintln!("Failed to save peer");
            } else {
                *jobs_available = true;
            }
        }
        SQLiteCommand::RemovePeer { peer_id } => {
            if do_remove_peer(conn, peer_id).is_err() {
                eprintln!("Failed to remove peer");
            }
        }
        SQLiteCommand::ResetJob { job_id } => {
            if do_reset_job(conn, job_id).is_err() {
                eprintln!("Failed to reset job");
            }
        }
    }

    Ok(())
}

pub fn init() -> Result<Connection> {
    let path = data_dir();
    std::fs::create_dir_all(&path)?;
    let db_path = path.join("ffmpeg-swarm.db");
    info!("Database running at: {}", db_path.display());
    let conn = Connection::open(db_path)?;
    conn.execute_batch(include_str!("./init.sql"))?;

    Ok(conn)
}
