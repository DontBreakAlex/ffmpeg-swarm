use anyhow::Result;
use directories::ProjectDirs;
use rusqlite::Connection;
use rusqlite::OptionalExtension;
use tokio::select;
use tokio::sync::mpsc::Permit;
use tokio::sync::mpsc::Sender;
use tokio::sync::{mpsc::Receiver, oneshot};

use crate::server::commands::{do_acquire_job, do_advertise, RunnableJob};
use crate::server::commands::do_save_peer;
use crate::server::commands::AdvertiseMessage;
use crate::{
	ipc::{Job, Task},
	server::commands::{do_complete, do_dispatch, do_submit, LocalJob},
};

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
	advertise_tx: Sender<Option<AdvertiseMessage>>,
	run_tx: Sender<RunnableJob>,
) -> Result<()> {
	let mut conn = init()?;
	let mut jobs_available = true;

	loop {
		select! {
            cmd = rx.recv() => {
                handle_cmd(cmd.expect("Database command sender not to be dropped"), &mut conn, &advertise_tx, &mut jobs_available).await?;
            }
            permit = run_tx.reserve(), if jobs_available => {
				if let Some(job) = do_acquire_job(&mut conn)? {
			        permit.expect("Failed to send job to run").send(job);
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
				// *jobs_available = true;
			}
		}
		SQLiteCommand::Dispatch { reply } => {
			if reply.send(do_dispatch(conn)).is_err() {
				eprintln!("Failed to send reply to dispatch command");
			}
		}
		SQLiteCommand::Complete { job, success } => {
			do_complete(conn, job, success)?;
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
	}

	Ok(())
}

pub fn find_job(permit: Permit<'_, RunnableJob>, conn: &mut Connection) -> Result<()> {
	let tx = conn.transaction()?;
	let mut get_job = tx.prepare_cached("SELECT jobs.id, task_id, inputs, output, args FROM jobs INNER JOIN tasks t on t.id = jobs.task_id WHERE jobs.started_at IS NULL ORDER BY jobs.created_at LIMIT 1;")?;
	let mut start_task = tx.prepare_cached(
		"UPDATE tasks SET started_at = coalesce(started_at, CURRENT_TIMESTAMP) WHERE id = ?;",
	)?;
	let mut start_job =
		tx.prepare_cached("UPDATE jobs SET started_at = CURRENT_TIMESTAMP WHERE id = ?;")?;
	let mut get_peer = tx.prepare_cached(
		"SELECT uuid, ips, oldest_job FROM known_peers ORDER BY oldest_job LIMIT 1;",
	)?;

	if let Some(local_job) = get_job.query_row([], LocalJob::from_row).optional()? {
		start_task.execute([local_job.task_id])?;
		start_job.execute([local_job.job_id])?;
		drop((get_job, start_task, start_job, get_peer));

		tx.commit()?;
		permit.send(RunnableJob::Local(local_job))
	}

	Ok(())
}

pub fn init() -> Result<Connection> {
	let dirs = ProjectDirs::from("none", "dontbreakalex", "ffmpeg-swarm").unwrap();
	let path = dirs.data_dir();
	std::fs::create_dir_all(&path)?;
	let db_path = path.join("ffmpeg-swarm.db");
	println!("Database running at: {}", db_path.display());
	let conn = Connection::open(db_path)?;
	conn.execute_batch(include_str!("./init.sql"))?;

	Ok(conn)
}
