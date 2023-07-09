pub mod commands;

use std::vec;
use std::{net::SocketAddr, time::Duration};

use anyhow::anyhow;
use anyhow::Result;
use directories::ProjectDirs;
use futures_lite::{AsyncReadExt, AsyncWriteExt};
use interprocess::local_socket::tokio::{LocalSocketListener, LocalSocketStream};
use interprocess::local_socket::NameTypeSupport;
use quinn::{Endpoint, ServerConfig, TransportConfig};
use rustls::{Certificate, PrivateKey};
use tokio::sync::mpsc::{self, Sender};

use crate::db::{loop_db, SQLiteCommand};
use crate::ipc::{CliToService, ServiceToCli};

const KEEP_ALIVE_SECS: u64 = 120;

pub fn run() -> Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    runtime.block_on(async move {
        let (tx, rx) = mpsc::channel(256);
        let cli_fut = tokio::spawn(loop_cli(tx.clone()));
        let quinn_fut = tokio::spawn(loop_quinn());
        let db_fut = tokio::spawn(loop_db(rx));

        println!("Server running");

        let e = tokio::select! {
            _ = cli_fut => anyhow!("CLI loop exited unexpectedly"),
            e = quinn_fut => anyhow!("QUIC loop exited unexpectedly {:#?}", e??),
            e = db_fut => anyhow!("DB loop exited unexpectedly {:#?}", e??),
        };

        Err(e)
    })
}

async fn loop_cli(tx: Sender<SQLiteCommand>) {
    let sock = {
        let name = {
            use NameTypeSupport::*;
            match NameTypeSupport::ALWAYS_AVAILABLE {
                OnlyPaths => "/tmp/ffmpeg-swarm.sock",
                OnlyNamespaced | Both => "@ffmpeg-swarm.sock",
            }
        };
        LocalSocketListener::bind(name).expect("Expected to bind to socket")
    };

    loop {
        let conn = match sock.accept().await {
            Ok(c) => c,
            Err(e) => {
                eprintln!("There was an error with an incoming connection: {}", e);
                continue;
            }
        };

        let tx = tx.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_cli(conn, tx).await {
                eprintln!("Error while handling connection: {}", e);
            }
        });
    }
}

async fn loop_quinn() -> Result<()> {
    let (cert, key) = read_or_generate_certs().unwrap();

    let mut transport = TransportConfig::default();
    transport.max_idle_timeout(Some(Duration::from_secs(KEEP_ALIVE_SECS).try_into()?));
    transport.keep_alive_interval(Some(Duration::from_secs(KEEP_ALIVE_SECS - 10).try_into()?));
    let mut server_config = ServerConfig::with_single_cert(vec![cert], key)?;
    server_config.transport_config(transport.into());

    let endpoint = Endpoint::server(server_config, "0.0.0.0:9753".parse::<SocketAddr>().unwrap())?;

    while let Some(conn) = endpoint.accept().await {
        tokio::spawn(async move {
            match conn.await {
                Ok(connection) => loop {
                    match connection.accept_bi().await {
                        Ok((_send, _recv)) => {
                            tokio::spawn(async move {});
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

fn read_or_generate_certs() -> Result<(Certificate, PrivateKey)> {
    let dirs = ProjectDirs::from("none", "dontbreakalex", "ffmpeg-swarm").unwrap();
    let path = dirs.data_dir();
    std::fs::create_dir_all(&path)?;
    let cert_path = path.join("cert.der");
    let key_path = path.join("key.der");

    if cert_path.exists() && key_path.exists() {
        let cert = std::fs::read(cert_path)?;
        let key = std::fs::read(key_path)?;
        Ok((Certificate(cert), PrivateKey(key)))
    } else {
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let key = cert.serialize_private_key_der();
        let cert = cert.serialize_der().unwrap();
        std::fs::write(cert_path, &cert)?;
        std::fs::write(key_path, &key)?;
        Ok((Certificate(cert), PrivateKey(key)))
    }
}

pub async fn handle_cli(conn: LocalSocketStream, tx: Sender<SQLiteCommand>) -> Result<()> {
    let (mut reader, mut writer) = conn.into_split();

    let mut buf = [0u8; 4];
    reader.read_exact(&mut buf).await?;
    let len = u32::from_le_bytes(buf) as usize;
    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf).await?;
    let cmd: CliToService = postcard::from_bytes(&buf)?;
    let res = match cmd {
        CliToService::SubmitJob { task, jobs } => commands::handle_submit(tx, task, jobs).await,
    };
    let vec =
        postcard::to_allocvec(&res.unwrap_or_else(|e| ServiceToCli::Error { e: e.to_string() }))?;
    let len = vec.len() as u32;
    writer.write_all(&len.to_le_bytes()).await?;
    writer.write_all(&vec).await?;

    Ok(())
}
