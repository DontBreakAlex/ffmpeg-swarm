use std::path::PathBuf;

use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::env;
use std::net::{IpAddr, SocketAddr};
use std::process::Stdio;
use tokio::sync::{oneshot, RwLock};

use anyhow::Result;
use directories::ProjectDirs;
use nix::sys::stat::Mode;
use nix::unistd::mkfifo;
use quinn::{ClientConfig, Connection, Endpoint};
use tokio::fs::{create_dir_all, remove_file, OpenOptions};
use tokio::io::{copy, AsyncWriteExt};
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio::task::JoinSet;
use tokio::{select, sync::mpsc::Sender};
use uuid::Uuid;

use crate::config::read_config;
use crate::inc::RequestMessage::RequestJob;
use crate::inc::StreamedJob;
use crate::server::commands::RunnableJob;
use crate::utils::read_or_generate_uuid;
use crate::{cli::parse::Arg, db::SQLiteCommand, ipc::Job};

use super::commands::{AdvertiseMessage, LocalJob};

type ConnectionPool = RwLock<HashMap<IpAddr, Connection>>;

pub async fn loop_run(
    tx: Sender<SQLiteCommand>,
    job_request_tx: mpsc::Sender<oneshot::Sender<RunnableJob>>,
) -> Result<()> {
    let endpoint = make_endpoint().await?;
    let conn_pool = ConnectionPool::default();
    let uuid = read_or_generate_uuid()?;
    let mut set = JoinSet::new();

    loop {
        while set.len()
            >= env::var("NUM_THREADS")
                .map(|s| s.parse().unwrap_or(1))
                .unwrap_or(1)
        {
            set.join_next().await;
        }

        let (s, r) = oneshot::channel();
        job_request_tx.send(s).await?;
        let Ok(job) = r.await else {
            println!("No job available");
            continue;
        };

        match job {
            RunnableJob::Remote(msg) => {
                let peer_id = msg.peer_id;
                if let Err(e) =
                    get_remote_job(&tx, &endpoint, msg, &conn_pool, *uuid, &mut set).await
                {
                    println!("Error getting remote job: {:?}", e);
                    tx.send(SQLiteCommand::RemovePeer { peer_id }).await?;
                }
            }
            RunnableJob::Local(j) => {
                let tx = tx.clone();
                set.spawn(async move {
                    if let Err(e) = run_job(&tx, j).await {
                        println!("Error running job: {:?}", e);
                    }
                });
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
        .map(|arg| match arg {
            Arg::Input => inputs
                .pop()
                .ok_or_else(|| anyhow::anyhow!("No input provided")),
            Arg::Output => Ok(output.clone()),
            Arg::Other(s) => Ok(s.into()),
        })
        .collect::<Result<Vec<PathBuf>, anyhow::Error>>()?;

    _ = create_dir_all(output.parent().unwrap()).await;

    println!("Running ffmpeg with args: {:?}", args);

    let mut child = Command::new("ffmpeg")
        .args(args)
        .stdin(Stdio::null())
        .spawn()?;

    let status = child.wait().await?;

    println!("ffmpeg exited with status: {}", status);

    tx.send(SQLiteCommand::Complete {
        job_id,
        exit_code: status.code().unwrap_or(-1),
        completed_by: Uuid::nil(),
    })
    .await?;

    Ok(())
}

async fn get_remote_job(
    tx: &Sender<SQLiteCommand>,
    endpoint: &Endpoint,
    msg: AdvertiseMessage,
    pool: &ConnectionPool,
    uuid: Uuid,
    set: &mut JoinSet<()>,
) -> Result<()> {
    let mut iter = msg.peer_ips.iter();
    let conn = 'a: loop {
        if let Some(ip) = iter.next() {
            if let Some(conn) = pool.read().await.get(ip).cloned() {
                if let Some(_) = conn.close_reason() {
                    pool.write().await.remove(ip);
                } else {
                    break conn;
                }
            }
        } else {
            let mut iter = msg.peer_ips.iter();
            loop {
                if let Some(ip) = iter.next() {
                    // We need to check again after acquiring the write lock because another thread could have opened the connection while we were checking other ips
                    match pool.write().await.entry(*ip) {
                        Entry::Occupied(c) => {
                            break 'a c.get().clone();
                        }
                        Entry::Vacant(entry) => {
                            if let Ok(conn) = endpoint.connect(SocketAddr::new(*ip, 9753), "swarm")
                            {
                                if let Ok(conn) = conn.await {
                                    entry.insert(conn.clone());
                                    break 'a conn;
                                }
                            }
                        }
                    }
                } else {
                    return Err(anyhow::anyhow!("Could not connect to peer"));
                }
            }
        }
    };

    println!("Connected to peer: {:?}", conn.remote_address());
    let (mut send, mut recv) = conn.open_bi().await?;
    send.write_all(
        &postcard::to_allocvec(&RequestJob {
            requester_uuid: uuid,
        })
        .unwrap(),
    )
    .await?;
    send.finish().await?;
    let job: Option<StreamedJob> = postcard::from_bytes(&recv.read_to_end(1_000_000).await?)?;
    if let Some(job) = job {
        set.spawn(async move {
            if let Err(e) = run_remote_job(conn, job, msg.peer_id).await {
                println!("Error running remote job: {:?}", e);
            }
        });
    } else {
        tx.send(SQLiteCommand::RemovePeer {
            peer_id: msg.peer_id,
        })
        .await?;
        println!("No job available");
    }

    Ok(())
}

pub async fn run_remote_job(conn: Connection, job: StreamedJob, peer_id: Uuid) -> Result<()> {
    let StreamedJob {
        args,
        input_count,
        extension,
        stream_id,
    } = job;
    let dirs = ProjectDirs::from("none", "dontbreakalex", "ffmpeg-swarm").unwrap();
    let runtime_dir = dirs.runtime_dir().unwrap();
    let mut set = JoinSet::new();

    let mut send = conn.open_uni().await?;
    let mut output_path = runtime_dir.join(format!("{peer_id}-{stream_id}-output"));
    output_path.set_extension(extension);
    remove_file(&output_path).await.unwrap_or(());
    mkfifo(&output_path, Mode::S_IRWXU)?;
    let _output_path = output_path.clone();
    set.spawn(async move {
        send.write_u64_le(stream_id).await?;
        let mut file = OpenOptions::new().read(true).open(_output_path).await?;
        copy(&mut file, &mut send).await?;
        send.finish().await?;
        Ok::<(), anyhow::Error>(())
    });
    let mut input_paths = Vec::new();
    for i in 0..input_count {
        let mut recv = conn.accept_uni().await?;
        let input_path = runtime_dir.join(format!("{peer_id}-{stream_id}-input-{i}"));
        remove_file(&input_path).await.unwrap_or(());
        mkfifo(&input_path, Mode::S_IRWXU)?;
        let _input_path = input_path.clone();
        set.spawn(async move {
            let mut file = OpenOptions::new().write(true).open(&_input_path).await?;
            copy(&mut recv, &mut file).await?;
            Ok::<(), anyhow::Error>(())
        });
        // let mut file = OpenOptions::new().write(true).create(true).open(&input_path).await?;
        // copy(&mut recv, &mut file).await?;
        // file.flush().await?;
        input_paths.push(input_path);
    }

    // TODO Use OsString instead of PathBuf
    let mut args: Vec<_> = args
        .into_iter()
        .map(|arg| match arg {
            Arg::Input => input_paths
                .pop()
                .ok_or_else(|| anyhow::anyhow!("No input provided")),
            Arg::Output => Ok(output_path.clone()),
            Arg::Other(s) => Ok(s.into()),
        })
        .collect::<Result<Vec<PathBuf>, anyhow::Error>>()?;
    args.push("-y".into());

    println!("Running ffmpeg with args: {:?}", args);

    let mut child = Command::new("ffmpeg")
        .args(args)
        .stdin(Stdio::null())
        .spawn()?;

    let mut run = true;

    loop {
        select! {
            task = set.join_next(), if run => {
                if let Some(result) = task {
                    result??;
                } else {
                    run = false;
                }
            },
            status = child.wait() => {
                let mut send = conn.open_uni().await?;
                send.write_u64_le(stream_id).await?;
                send.write_i32(status.map(|e| e.code().unwrap_or(-1i32)).unwrap_or(-1i32)).await?;
                send.finish().await?;
                break;
            }
        }
    }

    while let Some(result) = set.join_next().await {
        result??;
    }

    remove_file(output_path).await?;
    for input_path in input_paths {
        remove_file(input_path).await?;
    }

    Ok::<(), anyhow::Error>(())
}

pub async fn make_endpoint() -> Result<Endpoint> {
    let mut endpoint = Endpoint::client("0.0.0.0:0".parse().unwrap()).unwrap();
    let cert = read_config().cert.clone();
    let mut certs = rustls::RootCertStore::empty();
    certs.add(&cert)?;
    let client_config = ClientConfig::with_root_certificates(certs);
    endpoint.set_default_client_config(client_config);

    Ok(endpoint)
}
