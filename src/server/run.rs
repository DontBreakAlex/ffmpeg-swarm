use std::path::PathBuf;
use std::process::ExitCode;
use std::{process::Stdio, time::Duration};
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::RwLock;

use anyhow::Result;
use directories::ProjectDirs;
use nix::sys::stat::Mode;
use nix::unistd::mkfifo;
use quinn::{ClientConfig, Connection, Endpoint};
use tokio::process::Command;
use tokio::{select, sync::mpsc::Sender, time::sleep};
use tokio::fs::File;
use tokio::io::copy;
use tokio::sync::mpsc::Receiver;
use tokio::task::JoinSet;
use uuid::Uuid;

use crate::{cli::parse::Arg, db::SQLiteCommand, ipc::Job};
use crate::inc::RequestMessage::RequestJob;
use crate::inc::StreamedJob;
use crate::server::commands::RunnableJob;
use crate::utils::read_or_generate_certs;

use super::commands::{handle_dispatch, LocalJob, AdvertiseMessage};

type ConnectionPool = RwLock<HashMap<IpAddr, Connection>>;

pub async fn loop_run(tx: Sender<SQLiteCommand>, mut run_rx: Receiver<RunnableJob>) -> Result<()> {
	let endpoint = make_endpoint().await?;
	let conn_pool = ConnectionPool::default();

	loop {
		let job = run_rx.recv().await.expect("run_rx not to be dropped");
		match job {
			RunnableJob::Remote(msg) => {}
			RunnableJob::Local(j) => {
				if let Err(e) = run_job(&tx, j).await {
					println!("Error running job: {:?}", e);
				}
			}
		}
	}
}

pub async fn run_job(tx: &Sender<SQLiteCommand>, job: LocalJob) -> Result<()> {
	let LocalJob {
		job_id, job, args, ..
	} = job;
	let Job { mut inputs, output } = job;

	let args: Vec<_> = args
		.into_iter()
		.rev() // Where are poping inputs from the end
		.map(|arg| match arg {
			Arg::Input(_) => inputs
				.pop()
				.ok_or_else(|| anyhow::anyhow!("No input provided")),
			Arg::Output => Ok(output.clone()),
			Arg::Other(s) => Ok(s.into()),
		})
		.collect::<Result<Vec<PathBuf>, anyhow::Error>>()?;

	println!("Running ffmpeg with args: {:?}", args);

	let mut child = Command::new("ffmpeg")
		.args(args)
		.stdin(Stdio::null())
		.spawn()?;

	let status = child.wait().await?;

	println!("ffmpeg exited with status: {}", status);

	tx.send(SQLiteCommand::Complete {
		job: job_id,
		success: status.success(),
	}).await?;

	Ok(())
}

async fn get_remote_job(endpoint: &Endpoint, msg: &AdvertiseMessage, pool: &ConnectionPool) -> Result<()> {
	let mut iter = msg.peer_ips.iter();
	let conn = 'a: loop {
		if let Some(ip) = iter.next() {
			if let Some(conn) = pool.read().unwrap().get(ip).cloned() {
				break conn;
			}
		} else {
			let mut iter = msg.peer_ips.iter();
			loop {
				if let Some(ip) = iter.next() {
					// We need to check again after acquiring the write lock because another thread could have opened the connection while we were checking other ips
					match pool.write().unwrap().entry(*ip) {
						Entry::Occupied(c) => { break 'a c.get().clone(); },
						Entry::Vacant(entry) => {
							if let Ok(conn) = endpoint.connect(SocketAddr::new(*ip, 9753), "localhost")?.await {
								entry.insert(conn.clone());
								break 'a conn;
							}
						}
					}
				} else {
					return Err(anyhow::anyhow!("Could not connect to peer"));
				}
			}
		}
	};

	let (mut send, mut recv)  = conn.open_bi().await?;
	send.write_all(&postcard::to_allocvec(&RequestJob).unwrap()).await?;
	send.finish().await?;
	let job: Option<StreamedJob> = postcard::from_bytes(&recv.read_to_end(1_000_000).await?)?;
	if let Some(job) = job {
		run_remote_job(conn, job, msg.peer_id).await?;
	} else {
		todo!()
	}

	Ok(())
}

pub async fn run_remote_job(conn: Connection, job: StreamedJob, peer_id: Uuid) -> Result<()> {
	let StreamedJob { args, input_count } = job;
	let dirs = ProjectDirs::from("none", "dontbreakalex", "ffmpeg-swarm").unwrap();
	let runtime_dir = dirs.runtime_dir().unwrap();
	let mut set = JoinSet::new();

	let (mut send, _) = conn.accept_bi().await?;
	let output_path = runtime_dir.join(format!("output"));
	mkfifo(&output_path, Mode::S_IRWXU)?;
	let file = File::open(&output_path);
	set.spawn(async move {
		let mut file = file.await?;
		copy(&mut file, &mut send).await?;
		send.finish().await?;
		Ok(())
	});
	let mut input_paths = Vec::new();
	for i in 0..input_count {
		let mut recv = conn.accept_uni().await?;
		let input_path = runtime_dir.join(format!("input-{i}"));
		mkfifo(&input_path, Mode::S_IRWXU)?;
		let file = File::open(&output_path);
		set.spawn(async move {
			let mut file = file.await?;
			copy(&mut recv, &mut file).await?;
			Ok(())
		});
		input_paths.push(input_path);
	}

	// TODO Use OsString instead of PathBuf
	let args: Vec<_> = args
		.into_iter()
		.rev() // Where are poping inputs from the end
		.map(|arg| match arg {
			Arg::Input(_) => input_paths
				.pop()
				.ok_or_else(|| anyhow::anyhow!("No input provided")),
			Arg::Output => Ok(output_path),
			Arg::Other(s) => Ok(s.into()),
		})
		.collect::<Result<Vec<PathBuf>, anyhow::Error>>()?;

	println!("Running ffmpeg with args: {:?}", args);

	let mut child = Command::new("ffmpeg")
		.args(args)
		.stdin(Stdio::null())
		.spawn()?;

	loop {
		select! {
			task = set.join_next() => {
				task?
			},
			status = child.wait() => {
				// do something
				break;
			}
		}

	}

	while let Some(result) = set.join_next().await {
		result??;
	}

	// println!("ffmpeg exited with status: {}", status)

	Ok(())
}

pub async fn make_endpoint() -> Result<Endpoint> {
	let mut endpoint = Endpoint::client("0.0.0.0:0".parse().unwrap()).unwrap();
	let (cert, _) = read_or_generate_certs()?;
	let mut certs = rustls::RootCertStore::empty();
	certs.add(&cert)?;
	let client_config = ClientConfig::with_root_certificates(certs);
	endpoint.set_default_client_config(client_config);

	Ok(endpoint)
}