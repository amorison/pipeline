use std::{fs, io, path::PathBuf};

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
    Client {
        /// Configuration file
        config: PathBuf,
    },
    /// Start pipeline server
    Server {
        /// Configuration file
        config: PathBuf,
    },
}

pub async fn main() -> io::Result<()> {
    let cli = Cli::parse();
    match &cli.command {
        Commands::Client { config } => {
            let toml_content = fs::read_to_string(config)?;
            let config = toml::from_str(&toml_content).unwrap();
            client::main(config).await
        }
        Commands::Server { config } => {
            let toml_content = fs::read_to_string(config)?;
            let config = toml::from_str(&toml_content).unwrap();
            server::main(config).await
        }
    }
}
