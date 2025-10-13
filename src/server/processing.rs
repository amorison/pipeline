use std::fs;

use serde::Deserialize;
use tokio::{io, process::Command};

use crate::{FileSpec, replace_os_strings, server::Config};

#[derive(Deserialize, Debug, PartialEq, Eq)]
#[serde(untagged)]
enum Step {
    Mkdir { create_directory: String },
    ExternalCommand(Vec<String>),
}

impl Step {
    async fn run(&self, file: &FileSpec, config: &Config) -> io::Result<()> {
        let server_path = config.path_of(file);
        let rel_dir = file.relative_directory();
        let replacements = [
            ("{hash}", file.hash().as_ref()),
            ("{server_path}", server_path.as_os_str()),
            ("{client_name}", file.client.as_ref()),
            ("{client_relative_directory}", rel_dir.as_os_str()),
            ("{client_file_stem}", file.file_stem()),
        ];

        match self {
            Step::Mkdir { create_directory } => {
                let dir = replace_os_strings(create_directory, replacements.into_iter());
                fs::create_dir_all(dir)
            }
            Step::ExternalCommand(segments) => {
                let mut processing = Command::new(&segments[0])
                    .args(
                        segments[1..]
                            .iter()
                            .map(|a| replace_os_strings(a, replacements.into_iter())),
                    )
                    .spawn()?;

                match processing.wait().await {
                    Ok(status) if status.success() => Ok(()),
                    Ok(status) => Err(io::Error::other(format!("failed with status {status:?}"))),
                    Err(err) => Err(err),
                }
            }
        }
    }
}

#[derive(Deserialize, Debug, PartialEq, Eq)]
#[serde(untagged)]
enum InnerProc {
    One(Step),
    List(Vec<Step>),
}

#[derive(Deserialize, Debug, PartialEq, Eq)]
pub(super) struct Processing(InnerProc);

impl Processing {
    pub(super) async fn run(&self, file: &FileSpec, config: &Config) -> io::Result<()> {
        match &self.0 {
            InnerProc::One(step) => step.run(file, config).await,
            InnerProc::List(steps) => {
                for step in steps {
                    step.run(file, config).await?;
                }
                Ok(())
            }
        }
    }
}
