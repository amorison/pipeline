use std::io;

use clap::{Parser, Subcommand};

use crate::{client, server};

/// Processing pipeline utility
#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start pipeline client
    Client,
    /// Start pipeline server
    Server,
}

pub async fn main() -> io::Result<()> {
    let cli = Cli::parse();
    match &cli.command {
        Commands::Client => client::main().await,
        Commands::Server => server::main().await,
    }
}
