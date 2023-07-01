use std::{time::Duration, net::SocketAddr};

use directories::ProjectDirs;
use interprocess::local_socket::NameTypeSupport;
use interprocess::local_socket::tokio::LocalSocketListener;
use quinn::{TransportConfig, ServerConfig, Endpoint};
use rustls::{Certificate, PrivateKey};
use anyhow::Result;
use crate::cli::handle_cli;

pub fn run() {
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    runtime.block_on(async move {
        println!("Server running");

        tokio::spawn(loop_cli());
        tokio::spawn(loop_quinn());
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
	transport.max_idle_timeout(Some(Duration::from_secs(120).try_into()?));
	let mut server_config = ServerConfig::with_single_cert(vec![cert], key)?;
	server_config.transport_config(transport.into());

	let endpoint = Endpoint::server(server_config, "0.0.0.0:6969".parse::<SocketAddr>().unwrap())?;

	while let Some(conn) = endpoint.accept().await {
		tokio::spawn(async move {
			match conn.await {
				Ok(connection) => {
					loop {
						match connection.accept_bi().await {
							Ok((send, recv)) => {
								tokio::spawn(async move {
								});
							}
							Err(e) => {
								eprintln!("{}", e);
								return;
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


fn read_or_generate_certs() -> Result<(Certificate, PrivateKey)> {
    let dirs = ProjectDirs::from("org", "quinn", "quinn-examples").unwrap();
    let path = dirs.data_local_dir();
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