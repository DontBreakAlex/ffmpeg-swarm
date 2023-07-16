use std::path::PathBuf;
use std::process::ExitCode;
use std::{process::Stdio, time::Duration};

use tokio::{sync::mpsc::Sender, time::sleep};
use tokio::process::Command;
use anyhow::Result;

use crate::{db::SQLiteCommand, ipc::Job, cli::parse::Arg};

use super::commands::{handle_dispatch, DispatchedJob};

pub async fn loop_run(tx: Sender<SQLiteCommand>) -> Result<()> {
    loop {
        let handle_dispatch = handle_dispatch(tx.clone()).await;
        // println!("handle_dispatch: {:?}", handle_dispatch);
        match handle_dispatch {
            Ok(j) => {
                if let Err(e) = run_job(&tx, j).await {
                    println!("Error running job: {:?}", e);
                }
            },
            Err(_) => sleep(Duration::from_secs(1)).await,
        }
    }
}

pub async fn run_job(tx: &Sender<SQLiteCommand>, job: DispatchedJob) -> Result<()> {
    let DispatchedJob { job_id, task_id, job, args } = job;
    let Job { mut inputs, output } = job;

    let args: Vec<_> = args.into_iter().map(|arg| match arg {
        Arg::Input(_) => inputs.pop().ok_or_else(|| anyhow::anyhow!("No input provided")),
        Arg::Output => Ok(output.clone()),
        Arg::Other(s) => Ok(s.into()),
    }).collect::<Result<Vec<PathBuf>, anyhow::Error>>()?;

    println!("Running ffmpeg with args: {:?}", args);
    
    let mut child = Command::new("ffmpeg")
        .args(args)
        .stdin(Stdio::null())
        .spawn()?;

    let status = child.wait().await?;

    println!("ffmpeg exited with status: {}", status);

    tx.send(SQLiteCommand::Complete { job: job_id, success: status.success() }).await?;

    Ok(())
} 