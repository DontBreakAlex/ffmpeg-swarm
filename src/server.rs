pub mod commands;
pub mod run;

use crate::db::{loop_db, SQLiteCommand};
use crate::inc::{RequestMessage, StreamedJob};
use crate::server::commands::LocalJob;
use crate::server::run::loop_run;
use crate::{cli, mqtt, utils};
use anyhow::anyhow;
use anyhow::Result;
use directories::ProjectDirs;
use futures::TryFutureExt;
use nix::sys::stat::Mode;
use nix::unistd::mkdir;
use postage::dispatch;
use postage::prelude::{Sink, Stream};
use quinn::{Connection, Endpoint, RecvStream, ServerConfig, TransportConfig};
use std::cell::Cell;
use std::collections::HashMap;
use std::fs::remove_dir_all;
use std::rc::Rc;
use std::sync::Arc;
use std::vec;
use std::{net::SocketAddr, time::Duration};
use tokio::fs::File;
use tokio::io::copy;
use tokio::io::AsyncReadExt;
use tokio::select;
use tokio::sync::broadcast;
use tokio::sync::mpsc::{self, Sender};
use tokio::task::JoinSet;

const KEEP_ALIVE_SECS: u64 = 120;

pub fn run() -> Result<()> {
    let dirs = ProjectDirs::from("none", "dontbreakalex", "ffmpeg-swarm").unwrap();
    let runtime_dir = dirs.runtime_dir().unwrap();
    _ = remove_dir_all(&runtime_dir);
    _ = mkdir(runtime_dir, Mode::S_IRWXU);

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
            e = run_fut => anyhow!("Run loop exited unexpectedly {:#?}", e??),
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
                Ok(connection) => {
                    let (mut stream_tx, _) = dispatch::channel(16);
                    loop {
                        select! {
                            biased;
                            stream = connection.accept_bi() => {
                                match stream {
                                    Ok((send, recv)) => {
                                        let _tx = _tx.clone();
                                        let _conn = connection.clone();
                                        tokio::spawn(
                                            handle_bi(send, recv, _tx, _conn, stream_tx.subscribe())
                                                .inspect_err(|e| eprintln!("{:?}", e)),
                                        );
                                    }
                                    Err(e) => {
                                        eprintln!("{}", e);
                                        return;
                                    }
                                }
                            }
                            stream = connection.accept_uni() => match stream {
                                Ok(mut recv) => {
                                    let Ok(stream_id) = recv.read_u64().await else { return };
                                    if let Err(e) = stream_tx.send((stream_id, recv)).await {
                                        eprintln!("Error sending stream: {}", e);
                                    }
                                }
                                Err(e) => {
                                    eprintln!("{}", e);
                                    return;
                                }
                            }
                        }
                    }
                }
                Err(e) => eprintln!("{}", e),
            }
        });
    }

    Ok(())
}

async fn handle_bi(
    send: quinn::SendStream,
    mut recv: RecvStream,
    tx: Sender<SQLiteCommand>,
    conn: Connection,
    stream_rx: dispatch::Receiver<(u64, RecvStream)>,
) -> Result<()> {
    let vec = recv.read_to_end(1_000_000).await?;
    let msg: RequestMessage = postcard::from_bytes(&vec)?;

    match msg {
        RequestMessage::RequestJob => handle_request_job(conn, send, tx, stream_rx).await?,
    }

    Ok(())
}

async fn handle_request_job(
    conn: Connection,
    mut send: quinn::SendStream,
    tx: Sender<SQLiteCommand>,
    mut stream_rx: dispatch::Receiver<(u64, RecvStream)>,
) -> Result<()> {
    println!("Conn id: {:?}", conn.stable_id());
    let Some(LocalJob { job_id, task_id, job, args }) = commands::handle_dispatch(&tx).await? else {
		send.write_all(&postcard::to_allocvec(&None::<StreamedJob>)?).await?;
		send.finish().await?;
		return Ok(());
	};
    let stream_id = ((job_id as u64) << 32) | task_id as u64;

    let mut set = JoinSet::new();
    let out = File::create(&job.output).await?;
    let _conn = conn.clone();
    // TODO: WILL fuck up when multiple clients are starting jobs at the same time
    set.spawn(receive_output(out, stream_rx.clone(), stream_id));

    // TODO: Make sure that this actually works when there are multiple inputs (probably not)
    for (i, input) in job.inputs.iter().enumerate() {
        let mut f = File::open(input).await?;
        let mut s = conn.open_uni().await?;
        set.spawn(async move {
            println!("Sending input {i}");
            copy(&mut f, &mut s).await?;
            println!("Sent input {i}");
            Ok(())
        });
    }

    let streamed_job = Some(StreamedJob {
        args,
        input_count: job.inputs.len(),
        extension: job.output.extension().unwrap().to_os_string(),
        stream_id,
    });

    send.write_all(&postcard::to_allocvec(&streamed_job)?)
        .await?;
    send.finish().await?;

    while let Some(result) = set.join_next().await {
        result??;
    }

    let mut r = conn.accept_uni().await?;
    println!("Conn id 2: {:?}", conn.stable_id());
    let mut buf = [0u8; 4];
    r.read_exact(&mut buf).await?;
    let exit_code = i32::from_le_bytes(buf);

    println!("Exit code: {}", exit_code);
    tx.send(SQLiteCommand::Complete { job_id, exit_code })
        .await?;

    Ok(())
}

async fn receive_output(
    mut file: File,
    mut stream_rx: dispatch::Receiver<(u64, RecvStream)>,
    stream_id: u64,
) -> Result<()> {
    let (_, mut stream) = stream_rx
        .find(|(id, _)| *id == stream_id)
        .recv()
        .await
        .unwrap();
    copy(&mut stream, &mut file).await?;
    Ok(())
}
