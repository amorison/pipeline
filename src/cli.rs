use std::{
    fs, io,
    path::{Path, PathBuf},
};

use clap::{Parser, Subcommand};
use serde::Deserialize;

use crate::{client, query_db, server};

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
    /// Print server database content
    Db,
}

#[derive(Subcommand)]
enum ConfKind {
    /// Print out client configuration
    Client {
        /// Print configuration to this file, otherwise stdout
        path: Option<PathBuf>,
    },
    /// Print out server configuration
    Server {
        /// Print configuration to this file, otherwise stdout
        path: Option<PathBuf>,
    },
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
            let (content, path) = match kind {
                ConfKind::Client { path } => (client::DEFAULT_TOML_CONF, path),
                ConfKind::Server { path } => (server::DEFAULT_TOML_CONF, path),
            };
            match path {
                Some(path) => fs::write(path, content)?,
                None => print!("{content}"),
            }
            Ok(())
        }
        Commands::Db => query_db::main().await,
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
