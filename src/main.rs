mod cli;
mod db;
mod server;
mod service;

use clap::{Parser, Subcommand};
use service::{install_service, uninstall_service};
use std::{thread, time::Duration};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[arg(long, default_value = "false")]
    server: bool,
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Install {},
    Uninstall {},
    #[command(trailing_var_arg = true)]
    Submit {
        args: Vec<String>,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    // println!("{:#?}", cli);

    if cli.server {
        server::run();
    }

    match cli.command {
        Some(Commands::Install {}) => install_service(),
        Some(Commands::Uninstall {}) => uninstall_service(),
        Some(Commands::Submit { args }) => {
            cli::submit(args)?;
        }
        None => loop {
            println!("Hello, world!");
            thread::sleep(Duration::from_secs(1));
        },
    }

    Ok(())
}
