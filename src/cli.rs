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
    /// Start and manage pipeline client
    Client {
        #[command(subcommand)]
        cmd: ClientCmd,
    },
    /// Start and manage pipeline server
    Server {
        #[command(subcommand)]
        cmd: ServerCmd,
    },
}

#[derive(Subcommand)]
enum ClientCmd {
    /// Start pipeline client
    Start {
        /// Configuration file
        config: PathBuf,
    },
    /// Print configuration example
    Config {
        /// Print configuration to this file, otherwise stdout
        path: Option<PathBuf>,
        /// Generate configuration with SSH tunnel
        #[arg(long)]
        ssh_tunnel: bool,
    },
}

#[derive(Subcommand)]
enum ServerCmd {
    /// Start pipeline server
    Start {
        /// Configuration file
        config: PathBuf,
    },
    /// Print configuration example
    Config {
        /// Print configuration to this file, otherwise stdout
        path: Option<PathBuf>,
    },
    /// List files in pipeline and their status
    List,
    /// Prune processed files on server
    Prune {
        /// Configuration file
        config: PathBuf,
        /// Actually remove processed files
        #[arg(long, short)]
        force: bool,
    },
    /// Change the status of a file in the pipeline
    Mark {
        /// Hash of the processed file to update
        hash: String,
        /// Desired status to set
        status: MarkStatus,
    },
}

#[derive(clap::ValueEnum, Copy, Clone)]
pub(crate) enum MarkStatus {
    Done,
    Failed,
}

fn conf_from_toml<T: for<'a> Deserialize<'a>>(path: &Path) -> io::Result<T> {
    let content = fs::read(path)?;
    toml::from_slice(&content)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))
}

fn read_conf_and_chdir<T: for<'a> Deserialize<'a>>(path: &Path) -> io::Result<T> {
    let config = conf_from_toml(path)?;
    let work_dir = path
        .parent()
        .expect("config file should have a parent folder");
    if work_dir != Path::new("") {
        std::env::set_current_dir(work_dir)?;
    }
    Ok(config)
}

async fn client_cli(cmd: ClientCmd) -> io::Result<()> {
    match cmd {
        ClientCmd::Start { config } => client::main(read_conf_and_chdir(&config)?).await,
        ClientCmd::Config { path, ssh_tunnel } => {
            let content: &str = if ssh_tunnel {
                client::TUNNEL_TOML_CONF.as_ref()
            } else {
                client::DEFAULT_TOML_CONF.as_ref()
            };
            match path {
                Some(path) => fs::write(path, content)?,
                None => print!("{content}"),
            }
            Ok(())
        }
    }
}

async fn server_cli(cmd: ServerCmd) -> io::Result<()> {
    match cmd {
        ServerCmd::Start { config } => server::main(read_conf_and_chdir(&config)?).await,
        ServerCmd::Config { path } => {
            let content = server::DEFAULT_TOML_CONF;
            match path {
                Some(path) => fs::write(path, content)?,
                None => print!("{content}"),
            }
            Ok(())
        }
        ServerCmd::List => server::list::main().await,
        ServerCmd::Prune { config, force } => {
            server::prune::main(read_conf_and_chdir(&config)?, force).await
        }
        ServerCmd::Mark { hash, status } => server::mark::main(hash, status).await,
    }
}

pub async fn main() -> io::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Client { cmd } => client_cli(cmd).await,
        Commands::Server { cmd } => server_cli(cmd).await,
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
