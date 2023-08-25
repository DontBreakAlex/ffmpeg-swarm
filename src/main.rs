mod cli;
mod config;
mod db;
mod inc;
mod ipc;
mod mqtt;
mod server;
mod service;
mod utils;

use crate::config::{generate_config, serialize_config, write_serialized_config};
use clap::{Parser, Subcommand};
use service::{install_service, uninstall_service};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Server,
    Install,
    Uninstall,
    Submit { args: Vec<String> },
    Configure,
    ShowToken,
    Join { token: String },
}

fn main() -> anyhow::Result<()> {
    // let args: Vec<String> = std::env::args().collect();
    // println!("args: {:?}", args);
    let cli = Cli::parse();

    match cli.command {
        Commands::Install => install_service(),
        Commands::Uninstall => uninstall_service(),
        Commands::Submit { args } => {
            cli::submit(args)?;
        }
        Commands::Server => server::run()?,
        Commands::Configure => generate_config()?,
        Commands::ShowToken => println!("{}", serialize_config()?),
        Commands::Join { token } => write_serialized_config(token)?,
    }

    Ok(())
}
