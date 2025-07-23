use std::{fs, io, path::PathBuf};

use clap::{Parser, Subcommand};
use serde::Serialize;

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
    /// Print out configuration example
    PrintConfig {
        #[command(subcommand)]
        kind: ConfKind,
    },
}

#[derive(Subcommand)]
enum ConfKind {
    /// Print out client configuration
    Client,
    /// Print out server configuration
    Server,
}

fn default_conf_as_toml<T>() -> String
where
    T: Serialize + Default,
{
    toml::to_string_pretty(&T::default()).unwrap()
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
        Commands::PrintConfig {
            kind: ConfKind::Client,
        } => {
            print!("{}", default_conf_as_toml::<client::Config>());
            Ok(())
        }
        Commands::PrintConfig {
            kind: ConfKind::Server,
        } => {
            print!("{}", default_conf_as_toml::<server::Config>());
            Ok(())
        }
    }
}
