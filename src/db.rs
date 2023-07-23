use anyhow::anyhow;
use anyhow::Result;
use chrono::NaiveDateTime;
use diesel::Connection;
use diesel_migrations::EmbeddedMigrations;
use diesel_migrations::embed_migrations;
use diesel_migrations::MigrationHarness;
use diesel::prelude::*;
use directories::ProjectDirs;
use tokio::sync::mpsc::Sender;
use tokio::sync::{mpsc::Receiver, oneshot};

use crate::models::Job;
use crate::server::commands::AdvertiseMessage;
use crate::server::commands::do_advertise;
use crate::server::commands::do_save_peer;
use crate::{
    ipc::{Task},
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
    }
}

pub async fn loop_db(mut rx: Receiver<SQLiteCommand>, tx: Sender<Option<AdvertiseMessage>>) -> Result<()> {
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
                if do_save_peer(&mut conn, message).is_err() {
                    eprintln!("Failed to save peer");
                }
            }
        }
    }
}

const MIGRATIONS: EmbeddedMigrations = embed_migrations!();

pub fn init() -> Result<SqliteConnection> {
    use crate::schema::jobs::dsl::*;

    let dirs = ProjectDirs::from("none", "dontbreakalex", "ffmpeg-swarm").unwrap();
    let path = dirs.data_dir();
    std::fs::create_dir_all(&path)?;
    let db_path = path.join("ffmpeg-swarm.db");
    println!("Database path is {:?}", db_path);
    let mut conn = SqliteConnection::establish(&db_path.to_str().expect("Database path to be a valid utf-8 str"))?;
    conn.run_pending_migrations(MIGRATIONS).map_err(anyhow::Error::msg)?;
    diesel::update(jobs)
        .set(started_at.eq(None::<NaiveDateTime>))
        .filter(finished_at.is_null())
        .execute(&mut conn)?;

    Ok(conn)
}
