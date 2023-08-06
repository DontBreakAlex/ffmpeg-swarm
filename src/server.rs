pub mod commands;
pub mod run;

use once_cell::sync::OnceCell;
use std::vec;
use std::{net::SocketAddr, time::Duration};

use anyhow::anyhow;
use anyhow::Result;
use base64::Engine;
use directories::ProjectDirs;
use futures::SinkExt;
use futures_lite::{AsyncReadExt, AsyncWriteExt};
use interprocess::local_socket::tokio::{LocalSocketListener, LocalSocketStream};
use interprocess::local_socket::NameTypeSupport;
use quinn::{Connection, Endpoint, SendStream, ServerConfig, TransportConfig};
use rustls::{Certificate, PrivateKey};
use sha2::Digest;
use tokio::fs::File;
use tokio::io::{copy, copy_buf};
use tokio::sync::mpsc::{self, Sender};
use tokio::task::JoinSet;
use uuid::Uuid;

use crate::db::{loop_db, SQLiteCommand};
use crate::ipc::{CliToService, ServiceToCli};
use crate::{cli, mqtt, utils};
use crate::inc::{RequestMessage, StreamedJob};
use crate::server::commands::LocalJob;
use crate::server::run::loop_run;

const KEEP_ALIVE_SECS: u64 = 120;

pub fn run() -> Result<()> {
	let runtime = tokio::runtime::Builder::new_current_thread()
		.enable_all()
		.build()
		.unwrap();

	runtime.block_on(async move {
		let (db_tx, db_rx) = mpsc::channel(256);
		let (advertise_tx, advertise_rx) = mpsc::channel(256);
		let (run_tx, run_rx) = mpsc::channel(1);

		let cli_fut = tokio::spawn(cli::loop_cli(db_tx.clone()));
		let quinn_fut = tokio::spawn(loop_quinn(db_tx.clone()));
		let db_fut = tokio::spawn(loop_db(db_rx, advertise_tx, run_tx));
		let run_fut = tokio::spawn(loop_run(db_tx.clone(), run_rx));
		let mqtt_fut = tokio::spawn(mqtt::loop_mqtt(db_tx, advertise_rx));

		println!("Server running");

		let e = tokio::select! {
            _ = cli_fut => anyhow!("CLI loop exited unexpectedly"),
            e = quinn_fut => anyhow!("QUIC loop exited unexpectedly {:#?}", e??),
            e = db_fut => anyhow!("DB loop exited unexpectedly {:#?}", e??),
            // e = run_fut => anyhow!("Run loop exited unexpectedly {:#?}", e??),
            e = mqtt_fut => anyhow!("MQTT loop exited unexpectedly {:#?}", e??),
        };

		Err(e)
	})
}

async fn loop_quinn(tx: Sender<SQLiteCommand>) -> Result<()> {
	let (cert, key) = utils::read_or_generate_certs().unwrap();

	let mut transport = TransportConfig::default();
	transport.max_idle_timeout(Some(Duration::from_secs(KEEP_ALIVE_SECS).try_into()?));
	transport.keep_alive_interval(Some(Duration::from_secs(KEEP_ALIVE_SECS - 10).try_into()?));
	let mut server_config = ServerConfig::with_single_cert(vec![cert], key)?;
	server_config.transport_config(transport.into());

	let endpoint = Endpoint::server(server_config, "0.0.0.0:9753".parse::<SocketAddr>().unwrap())?;

	while let Some(conn) = endpoint.accept().await {
		let _tx = tx.clone();
		tokio::spawn(async move {
			match conn.await {
				Ok(connection) => loop {
					match connection.accept_bi().await {
						Ok((send, recv)) => {
							let _tx = _tx.clone();
							let _conn = connection.clone();
							tokio::spawn(handle_bi(send, recv, _tx, _conn));
						}
						Err(e) => {
							eprintln!("{}", e);
							return;
						}
					}
				},
				Err(e) => eprintln!("{}", e),
			}
		});
	}

	Ok(())
}

async fn handle_bi(mut send: quinn::SendStream, mut recv: quinn::RecvStream, tx: Sender<SQLiteCommand>, conn: Connection) -> Result<()> {
	let vec = recv.read_to_end(1_000_000).await?;
	let msg: RequestMessage = postcard::from_bytes(&vec)?;

	match msg {
		RequestMessage::RequestJob => handle_request_job(conn, send, tx).await?,
	}

	Ok(())
}

async fn handle_request_job(conn: Connection, mut send: quinn::SendStream, tx: Sender<SQLiteCommand>) -> Result<()> {
	let Some(LocalJob { job_id, task_id, job, args }) = commands::handle_dispatch(tx).await? else {
		send.write_all(&postcard::to_allocvec(&None::<StreamedJob>)?).await?;
		send.finish().await?;
		return Ok(());
	};

	let mut set = JoinSet::new();
	let mut out = File::open(&job.output).await?;
	let (s, mut r) = conn.open_bi().await?;
	set.spawn(async move {
		copy(&mut r, &mut out).await?;
		Ok::<(), tokio::io::Error>(())
	});

	for (i, input) in job.inputs.iter().enumerate() {
		let mut f = File::open(input).await?;
		let mut s = conn.open_uni().await?;
		set.spawn(async move {
			s.write(&[i as u8]).await?;
			copy(&mut f, &mut s).await?;
			Ok(())
		});
	}

	let streamed_job = Some(StreamedJob {
		args,
		input_count: job.inputs.len(),
	});

	send.write_all(&postcard::to_allocvec(&streamed_job)?).await?;
	send.finish().await?;

	while let Some(result) = set.join_next().await {
		result??;
	}

	Ok(())
}