mod service;

use std::{thread, time::Duration};
use clap::{Parser, Subcommand};
use service::{install_service, uninstall_service};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    Install {},
    Uninstall {},
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Install {}) => install_service(),
        Some(Commands::Uninstall {}) => uninstall_service(),
        None => loop {
            println!("Hello, world!");
            thread::sleep(Duration::from_secs(1));
        },
    }
}
