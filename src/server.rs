pub mod commands;
pub mod run;

use crate::db::{loop_db, SQLiteCommand};
use crate::inc::{RequestMessage, StreamedJob};
use crate::server::commands::LocalJob;
use crate::server::run::loop_run;
use crate::{cli, mqtt};
use anyhow::Result;
use anyhow::{anyhow, Context};
use atomic_take::AtomicTake;
use directories::ProjectDirs;
use futures::TryFutureExt;
use nix::sys::stat::Mode;
use nix::unistd::mkdir;
use quinn::{Connection, Endpoint, RecvStream, ServerConfig, TransportConfig};

use std::fs::remove_dir_all;

use crate::config::read_config;
use std::sync::Arc;
use std::vec;
use std::{net::SocketAddr, time::Duration};
use tokio::fs::{create_dir_all, File};
use tokio::io::copy;
use tokio::io::AsyncReadExt;
use tokio::select;
use tokio::sync::broadcast;
use tokio::sync::mpsc::{self, channel, Sender};
use tokio::task::JoinSet;
use uuid::Uuid;

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
        let (run_tx, run_rx) = channel(1);

        let cli_fut = tokio::spawn(cli::loop_cli(db_tx.clone()));
        let quinn_fut = tokio::spawn(loop_quinn(db_tx.clone()));
        let db_fut = tokio::spawn(loop_db(db_rx, advertise_tx, run_rx));
        let run_fut = tokio::spawn(loop_run(db_tx.clone(), run_tx));
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
    let config = read_config();
    let mut transport = TransportConfig::default();
    transport.max_idle_timeout(Some(Duration::from_secs(KEEP_ALIVE_SECS).try_into()?));
    transport.keep_alive_interval(Some(Duration::from_secs(KEEP_ALIVE_SECS - 10).try_into()?));
    let mut server_config =
        ServerConfig::with_single_cert(vec![config.cert.clone()], config.key.clone())?;
    server_config.transport_config(transport.into());

    let endpoint = Endpoint::server(server_config, "0.0.0.0:9753".parse::<SocketAddr>().unwrap())?;

    while let Some(conn) = endpoint.accept().await {
        let _tx = tx.clone();
        tokio::spawn(async move {
            match conn.await {
                Ok(connection) => {
                    let (stream_tx, _) = broadcast::channel(16);
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
                                    let Ok(stream_id) = recv.read_u64_le().await else { return };

                                    if let Err(e) = stream_tx.send((stream_id, Arc::new(AtomicTake::new(recv)))) {
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
    stream_rx: broadcast::Receiver<(u64, Arc<AtomicTake<RecvStream>>)>,
) -> Result<()> {
    let vec = recv.read_to_end(1_000_000).await?;
    let msg: RequestMessage = postcard::from_bytes(&vec)?;

    match msg {
        RequestMessage::RequestJob { requester_uuid } => {
            handle_request_job(conn, send, tx, stream_rx, requester_uuid).await?
        }
    }

    Ok(())
}

async fn handle_request_job(
    conn: Connection,
    mut send: quinn::SendStream,
    tx: Sender<SQLiteCommand>,
    stream_rx: broadcast::Receiver<(u64, Arc<AtomicTake<RecvStream>>)>,
    requester_uuid: Uuid,
) -> Result<()> {
    let Some(LocalJob {
        job_id,
        task_id,
        job,
        args,
    }) = commands::handle_dispatch(&tx).await?
    else {
        send.write_all(&postcard::to_allocvec(&None::<StreamedJob>)?)
            .await?;
        send.finish().await?;
        return Ok(());
    };
    let stream_id = ((job_id as u64) << 32) | task_id as u64;

    let mut set: JoinSet<Result<()>> = JoinSet::new();
    _ = create_dir_all(&job.output.parent().unwrap()).await;
    let out = File::create(&job.output).await?;
    let _conn = conn.clone();
    let new_stream_rx = stream_rx.resubscribe();
    set.spawn(receive_output(out, stream_rx, stream_id));

    // TODO: Make sure that this actually works when there are multiple inputs (probably not)
    for input in job.inputs.iter() {
        let mut f = File::open(input).await?;
        let mut s = conn.open_uni().await?;
        set.spawn(async move {
            copy(&mut f, &mut s).await?;
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

    let exit_code = receive_exit_code(new_stream_rx, stream_id).await?;

    println!("Job {job_id} from task {task_id} exited with code {exit_code}");

    tx.send(SQLiteCommand::Complete {
        job_id,
        exit_code,
        completed_by: requester_uuid,
    })
    .await?;

    Ok(())
}

async fn receive_output(
    mut file: File,
    mut stream_rx: broadcast::Receiver<(u64, Arc<AtomicTake<RecvStream>>)>,
    stream_id: u64,
) -> Result<()> {
    loop {
        let (id, stream) = stream_rx.recv().await?;
        if id == stream_id {
            let mut stream = stream.take().unwrap();
            copy(&mut stream, &mut file).await?;
            return Ok(());
        }
    }
}

async fn receive_exit_code(
    mut stream_rx: broadcast::Receiver<(u64, Arc<AtomicTake<RecvStream>>)>,
    stream_id: u64,
) -> Result<i32> {
    loop {
        let (id, stream) = stream_rx.recv().await?;
        if id == stream_id {
            if let Some(mut stream) = stream.take() {
                return Ok(stream.read_i32_le().await?);
            }
        }
    }
}
