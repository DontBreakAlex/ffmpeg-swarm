pub mod commands;
mod run;

use once_cell::sync::OnceCell;
use std::net::IpAddr;
use std::vec;
use std::{net::SocketAddr, time::Duration};

use anyhow::anyhow;
use anyhow::Result;
use base64::engine::general_purpose;
use base64::Engine;
use directories::ProjectDirs;
use futures_lite::{AsyncReadExt, AsyncWriteExt};
use interprocess::local_socket::tokio::{LocalSocketListener, LocalSocketStream};
use interprocess::local_socket::NameTypeSupport;
use local_ip_address::list_afinet_netifas;
use quinn::{Endpoint, ServerConfig, TransportConfig};
use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};
use rustls::{Certificate, PrivateKey};
use sha2::{Digest, Sha256};
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio::time::{sleep, timeout};
use uuid::Uuid;

use crate::db::{loop_db, SQLiteCommand};
use crate::ipc::{CliToService, ServiceToCli};
use crate::server::run::loop_run;

use self::commands::AdvertiseMessage;

const KEEP_ALIVE_SECS: u64 = 120;

pub fn run() -> Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    runtime.block_on(async move {
        let (db_tx, db_rx) = mpsc::channel(256);
        let (advertise_tx, advertise_rx) = mpsc::channel(256);
        let cli_fut = tokio::spawn(loop_cli(db_tx.clone()));
        let quinn_fut = tokio::spawn(loop_quinn());
        let db_fut = tokio::spawn(loop_db(db_rx, advertise_tx));
        // let run_fut = tokio::spawn(loop_run(db_tx.clone()));
        let mqtt_fut = tokio::spawn(loop_mqtt(db_tx, advertise_rx));

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

fn read_or_generate_uuid() -> Result<&'static Uuid> {
    static UUID: OnceCell<Uuid> = OnceCell::new();
    UUID.get_or_try_init(|| {
        let dirs = ProjectDirs::from("none", "dontbreakalex", "ffmpeg-swarm").unwrap();
        let path = dirs.data_dir();
        std::fs::create_dir_all(&path)?;
        let uuid_path = path.join("uuid");

        if uuid_path.exists() {
            let uuid = std::fs::read_to_string(uuid_path)?;
            Ok(Uuid::parse_str(&uuid)?)
        } else {
            let uuid = Uuid::new_v4();
            std::fs::write(uuid_path, uuid.to_string())?;
            Ok(uuid)
        }
    })
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

async fn loop_mqtt(
    tx: Sender<SQLiteCommand>,
    mut rx: Receiver<Option<AdvertiseMessage>>,
) -> Result<()> {
    let uuid = read_or_generate_uuid()?;
    let topic = gen_topic()?;
    let mut mqttoptions = MqttOptions::new(
        read_or_generate_uuid()?.to_string(),
        "test.mosquitto.org",
        1883,
    );
    mqttoptions.set_keep_alive(Duration::from_secs(30));

    let (mut client, mut eventloop) = AsyncClient::new(mqttoptions, 10);
    client.subscribe(&topic, QoS::AtLeastOnce).await.unwrap();

    let ips: Vec<IpAddr> = list_afinet_netifas()?
        .into_iter()
        .filter(|(_, ip)| !ip.is_loopback())
        .map(|i| i.1)
        .collect();

    let _tx = tx.clone();
    tokio::spawn(async move {
        while let Ok(notification) = eventloop.poll().await {
            println!("Received = {:?}", notification);
            match notification {
                Event::Incoming(packet) => match packet {
                    Packet::Publish(p) => {
                        let Ok(msg) = postcard::from_bytes::<AdvertiseMessage>(&p.payload) else { continue };
                        println!("Received = {:?}", msg);
                        if msg.peer_id == *uuid {
                            continue;
                        }
                        if let Err(e) = _tx.send(SQLiteCommand::SavePeer { message: msg }).await {
                            eprintln!("Failed to save peer: {}", e);
                        }
                    }
                    _ => {}
                },
                Event::Outgoing(_) => {}
            }
        }
    });

    loop {
        let msg = timeout(Duration::from_secs(20), rx.recv()).await;
        let mut msg = match msg {
            Ok(Some(Some(msg))) => msg,
            Ok(None) => unreachable!(),
            Ok(Some(None)) => continue,
            Err(_) => {
                tx.send(SQLiteCommand::Advertise).await?;
                continue;
            }
        };
        msg.peer_ips = ips.clone();

        client
            .publish(&topic, QoS::AtLeastOnce, true, postcard::to_allocvec(&msg)?)
            .await
            .unwrap();
    }

    Ok(())
}

fn gen_topic() -> Result<String> {
    let (cert, _) = read_or_generate_certs()?;
    let mut hasher = Sha256::new();
    hasher.update(&cert.0);
    let cert_hash = hasher.finalize();
    let topic = format!(
        "ffmpeg-swarm/{}",
        general_purpose::URL_SAFE_NO_PAD.encode(cert_hash)
    );
    Ok(topic)
}
