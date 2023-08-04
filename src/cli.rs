mod ipc;
pub mod parse;
mod validation;

use crate::{cli::{
	parse::{FfmpegArgs, parse_ffmpeg_args},
	validation::validate_files,
}, ipc::{CliToService, Task}, server};
use anyhow::Result;
use ipc::send_command;
use tokio::sync::mpsc::Sender;
use interprocess::local_socket::tokio::{LocalSocketListener, LocalSocketStream};
use futures_lite::{AsyncReadExt, AsyncWriteExt};
use crate::db::SQLiteCommand;
use crate::ipc::ServiceToCli;
use crate::server::commands;

pub fn submit(args: Vec<String>) -> Result<()> {
    let FfmpegArgs {
        input,
        output,
        args,
    } = parse_ffmpeg_args(args)?;
    let task = Task { args };
    let jobs = validate_files(input, output)?;
    let res = send_command(CliToService::SubmitJob { task, jobs })?;
    println!("{:?} ---", res);
    Ok(())
}

pub async fn loop_cli(tx: Sender<SQLiteCommand>) {
    let sock = {
        let name = {
            use interprocess::local_socket::NameTypeSupport::*;
	        use interprocess::local_socket::NameTypeSupport;
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
