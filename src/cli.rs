use std::{
    fs, io,
    path::{Path, PathBuf},
};

use clap::{Parser, Subcommand};
use serde::Deserialize;

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

fn conf_from_toml<T: for<'a> Deserialize<'a>>(path: &Path) -> io::Result<T> {
    let content = fs::read(path)?;
    toml::from_slice(&content)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))
}

pub async fn main() -> io::Result<()> {
    let cli = Cli::parse();
    match &cli.command {
        Commands::Client { config } => client::main(conf_from_toml(config)?).await,
        Commands::Server { config } => server::main(conf_from_toml(config)?).await,
        Commands::PrintConfig { kind } => {
            let content = match kind {
                ConfKind::Client => client::DEFAULT_TOML_CONF,
                ConfKind::Server => server::DEFAULT_TOML_CONF,
            };
            print!("{content}");
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use super::*;

    #[test]
    fn verify_cli() {
        Cli::command().debug_assert();
    }
}
