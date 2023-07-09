use std::vec;
use std::{net::SocketAddr, time::Duration};

use anyhow::Result;
use directories::ProjectDirs;
use futures_lite::{AsyncReadExt, AsyncWriteExt};
use interprocess::local_socket::tokio::{LocalSocketListener, LocalSocketStream};
use interprocess::local_socket::NameTypeSupport;
use quinn::{Endpoint, ServerConfig, TransportConfig};
use rustls::{Certificate, PrivateKey};

use crate::ipc::{CliToService, Job, ServiceToCli, Task};

const KEEP_ALIVE_SECS: u64 = 120;

pub fn run() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async move {
        let cli_fut = tokio::spawn(loop_cli());
        let quinn_fut = tokio::spawn(loop_quinn());

        println!("Server running");

        tokio::select! {
            _ = cli_fut => {},
            _ = quinn_fut => {},
        }
    });
}

async fn loop_cli() {
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

        tokio::spawn(async move {
            if let Err(e) = handle_cli(conn).await {
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
    let dirs = ProjectDirs::from("org", "dontbreakalex", "ffmpeg-swarm").unwrap();
    let path = dirs.data_local_dir();
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

pub async fn handle_cli(conn: LocalSocketStream) -> Result<()> {
    let (mut reader, mut writer) = conn.into_split();

    let mut buf = [0u8; 4];
    reader.read_exact(&mut buf).await?;
    let len = u32::from_le_bytes(buf) as usize;
    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf).await?;
    let cmd: CliToService = postcard::from_bytes(&buf)?;
    let CliToService::SubmitJob { task, jobs } = cmd;
    println!("Received task: {:?}", task);
    println!("Received job: {:?}", jobs);
    let vec = postcard::to_allocvec(&ServiceToCli::Ok)?;
    let len = vec.len() as u32;
    writer.write_all(&len.to_le_bytes()).await?;
    writer.write_all(&vec).await?;

    Ok(())
}
