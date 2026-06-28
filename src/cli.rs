use std::{
    fs, io,
    path::{Path, PathBuf},
};

use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};

use crate::{
    client,
    server::{
        self,
        query::{self, Query},
    },
};

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
    /// Query the pipeline server
    Query {
        #[command(subcommand)]
        cmd: QueryCmd,
    },
}

#[derive(Subcommand)]
enum ClientCmd {
    /// Start pipeline client
    Start {
        /// Configuration file
        config: PathBuf,
    },
    /// Start pipeline client, stopping as soon as no new files are found
    StartOnce {
        /// Configuration file
        config: PathBuf,
    },
    /// List files that would be processed in watched directory
    WatchedFiles {
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
    /// Remove already processed files on server
    Clean {
        /// Configuration file
        config: PathBuf,
        /// Also remove `Done` tasks instead of only `ToPrune` ones
        #[arg(long)]
        done: bool,
    },
    /// Convenience to create all buckets, e.g. to set permissions
    CreateBuckets {
        /// Configuration file
        config: PathBuf,
    },
}

#[derive(Subcommand)]
enum QueryCmd {
    /// List files in pipeline and their status
    List {
        /// Configuration file
        config: PathBuf,
    },
    /// Change the status of a file in the pipeline
    Mark {
        /// Configuration file
        config: PathBuf,
        /// Hash of the processed file to update
        hash: String,
        /// Desired status to set
        status: MarkStatus,
    },
}

#[derive(clap::ValueEnum, Serialize, Deserialize, Copy, Clone, Debug)]
pub(crate) enum MarkStatus {
    Done,
    Failed,
    ToPrune,
}

fn conf_from_toml<T: for<'a> Deserialize<'a>>(path: &Path) -> io::Result<T> {
    let content = fs::read(path)?;
    match toml::from_slice(&content) {
        Ok(conf) => Ok(conf),
        Err(err) => {
            eprintln!("{err}");
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid config file",
            ))
        }
    }
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
        ClientCmd::Start { config } => client::main(read_conf_and_chdir(&config)?, false).await,
        ClientCmd::StartOnce { config } => client::main(read_conf_and_chdir(&config)?, true).await,
        ClientCmd::WatchedFiles { config } => {
            client::watch::main(read_conf_and_chdir(&config)?).await
        }
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
        ServerCmd::Clean { config, done } => {
            server::clean::main(read_conf_and_chdir(&config)?, done).await
        }
        ServerCmd::CreateBuckets { config } => {
            server::create_buckets::main(read_conf_and_chdir(&config)?).await
        }
    }
}

async fn query_cli(cmd: QueryCmd) -> io::Result<()> {
    match cmd {
        QueryCmd::List { config } => {
            let config = read_conf_and_chdir(&config)?;
            query::main(config, Query::List).await
        }
        QueryCmd::Mark {
            config,
            hash,
            status,
        } => {
            let config = read_conf_and_chdir(&config)?;
            let query = Query::Mark { hash, status };
            query::main(config, query).await
        }
    }
}

pub async fn main() -> io::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Client { cmd } => client_cli(cmd).await,
        Commands::Server { cmd } => server_cli(cmd).await,
        Commands::Query { cmd } => query_cli(cmd).await,
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
