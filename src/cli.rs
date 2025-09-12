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
    /// Prune processed files on server
    Prune {
        /// Configuration file
        config: PathBuf,
        /// Actually remove processed files
        #[arg(long, short)]
        force: bool,
    },
}

#[derive(Subcommand)]
enum ConfKind {
    /// Print out client configuration
    Client {
        /// Print configuration to this file, otherwise stdout
        path: Option<PathBuf>,
        /// Configuration with SSH tunnel
        #[arg(long)]
        ssh_tunnel: bool,
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

fn read_conf_and_chdir<T: for<'a> Deserialize<'a>>(path: &Path) -> io::Result<T> {
    let path = path.canonicalize()?;
    let config = conf_from_toml(&path)?;
    let work_dir = path
        .parent()
        .expect("config file should have a parent folder");
    std::env::set_current_dir(work_dir)?;
    Ok(config)
}

pub async fn main() -> io::Result<()> {
    let cli = Cli::parse();
    match &cli.command {
        Commands::Client { config } => client::main(read_conf_and_chdir(config)?).await,
        Commands::Server { config } => server::main(read_conf_and_chdir(config)?).await,
        Commands::PrintConfig { kind } => {
            let (content, path) = match kind {
                ConfKind::Client {
                    path,
                    ssh_tunnel: false,
                } => (client::DEFAULT_TOML_CONF.as_ref(), path),
                ConfKind::Client {
                    path,
                    ssh_tunnel: true,
                } => (client::TUNNEL_TOML_CONF.as_ref(), path),
                ConfKind::Server { path } => (server::DEFAULT_TOML_CONF, path),
            };
            match path {
                Some(path) => fs::write(path, content)?,
                None => print!("{content}"),
            }
            Ok(())
        }
        Commands::Db => query_db::main().await,
        Commands::Prune { config, force } => {
            server::prune::main(read_conf_and_chdir(config)?, *force).await
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
