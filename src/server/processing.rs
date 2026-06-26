use std::{
    ffi::{OsStr, OsString},
    fs,
    path::PathBuf,
};

use serde::Deserialize;
use tokio::{io, process::Command};

use crate::{FileSpec, custom_serde, replace_os_strings, server::Config};

struct Replacements<'a> {
    file: &'a FileSpec,
    server_path: PathBuf,
    rel_dir: PathBuf,
}

impl<'a> Replacements<'a> {
    fn new(file: &'a FileSpec, config: &Config) -> Self {
        Self {
            file,
            server_path: config.path_of(file),
            rel_dir: file.relative_directory(),
        }
    }

    fn iter(&'a self) -> impl Iterator<Item = (&'a str, &'a OsStr)> {
        [
            ("{hash}", self.file.hash().as_ref()),
            ("{server_path}", self.server_path.as_os_str()),
            ("{client_name}", self.file.client.as_ref()),
            ("{client_relative_directory}", self.rel_dir.as_os_str()),
            ("{client_file_stem}", self.file.file_stem()),
        ]
        .into_iter()
    }

    fn apply_to(&'a self, s: &str) -> OsString {
        replace_os_strings(s, self.iter())
    }
}

#[derive(Deserialize, Debug, PartialEq, Eq)]
#[serde(untagged)]
enum Step {
    Mkdir { create_directory: String },
    DeleteFile { delete_file: String },
    DeleteDirectory { delete_directory: String },
    ExternalCommand(#[serde(deserialize_with = "custom_serde::vec_at_least_one")] Vec<String>),
}

impl Step {
    async fn run(&self, file: &FileSpec, config: &Config) -> io::Result<()> {
        let rep = Replacements::new(file, config);
        match self {
            Step::Mkdir { create_directory } => {
                let dir = rep.apply_to(create_directory);
                fs::create_dir_all(dir)
            }
            Step::DeleteFile { delete_file } => {
                let path = rep.apply_to(delete_file);
                fs::remove_file(path)
            }
            Step::DeleteDirectory { delete_directory } => {
                let path = rep.apply_to(delete_directory);
                fs::remove_dir_all(path)
            }
            Step::ExternalCommand(segments) => {
                let mut processing = Command::new(&segments[0])
                    .args(segments[1..].iter().map(|a| rep.apply_to(a)))
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
enum InnerProc {
    #[serde(rename = "pass")]
    Pass,
    #[serde(untagged)]
    One(Step),
    #[serde(untagged)]
    List(#[serde(deserialize_with = "custom_serde::vec_at_least_one")] Vec<Step>),
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
            InnerProc::Pass => Ok(()),
        }
    }
}
