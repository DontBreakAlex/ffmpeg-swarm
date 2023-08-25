use crate::config::read_config;
use crate::db::SQLiteCommand;
use crate::server::commands::AdvertiseMessage;
use crate::utils;
use base64::engine::general_purpose;
use base64::Engine;
use local_ip_address::list_afinet_netifas;
use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};
use sha2::{Digest, Sha256};
use std::net::IpAddr;
use std::time::Duration;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::time::timeout;

pub async fn loop_mqtt(
    tx: Sender<SQLiteCommand>,
    mut rx: Receiver<Option<AdvertiseMessage>>,
) -> anyhow::Result<()> {
    let uuid = utils::read_or_generate_uuid()?;
    let topic = gen_topic()?;
    let mut mqttoptions = MqttOptions::new(
        utils::read_or_generate_uuid()?.to_string(),
        read_config().mqtt.clone(),
        1883,
    );
    mqttoptions.set_keep_alive(Duration::from_secs(30));

    let (client, mut eventloop) = AsyncClient::new(mqttoptions, 10);
    client.subscribe(&topic, QoS::AtLeastOnce).await.unwrap();

    let ips: Vec<IpAddr> = list_afinet_netifas()?
        .into_iter()
        .filter(|(_, ip)| !ip.is_loopback() && ip.is_ipv4())
        .map(|i| i.1)
        .collect();

    let _tx = tx.clone();
    tokio::spawn(async move {
        while let Ok(notification) = eventloop.poll().await {
            match notification {
                Event::Incoming(packet) => match packet {
                    Packet::Publish(p) => {
                        let Ok(msg) = postcard::from_bytes::<AdvertiseMessage>(&p.payload) else {
                            continue;
                        };
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
            .publish(
                &topic,
                QoS::AtLeastOnce,
                false,
                postcard::to_allocvec(&msg)?,
            )
            .await
            .unwrap();
    }
}

fn gen_topic() -> anyhow::Result<String> {
    let cert = &read_config().cert;
    let mut hasher = Sha256::new();
    hasher.update(&cert.0);
    let cert_hash = hasher.finalize();
    let topic = format!(
        "ffmpeg-swarm/{}",
        general_purpose::URL_SAFE_NO_PAD.encode(cert_hash)
    );
    Ok(topic)
}
