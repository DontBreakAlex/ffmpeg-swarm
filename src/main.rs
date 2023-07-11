mod cli;
mod db;
mod ipc;
mod server;
mod service;

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
    #[command(trailing_var_arg = true)]
    Submit {
        args: Vec<String>,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Install => install_service(),
        Commands::Uninstall => uninstall_service(),
        Commands::Submit { args } => {
            cli::submit(args)?;
        }
        Commands::Server => server::run()?,
    }

    Ok(())
}
