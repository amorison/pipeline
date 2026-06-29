use std::{
    ffi::{OsStr, OsString},
    fs,
    path::PathBuf,
};

use log::warn;
use serde::Deserialize;
use tokio::{io, process::Command};

use crate::{
    FileSpec, custom_serde, replace_os_strings,
    server::{Config, Database, ProcessStatus},
};

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
            ("{client_file_name}", self.file.filename.as_ref()),
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
    async fn run(&self, rep: &Replacements<'_>) -> io::Result<()> {
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

#[derive(Deserialize, Debug, PartialEq, Eq)]
pub(super) enum AfterProcessing {
    #[serde(rename = "pass")]
    Pass,
    #[serde(untagged)]
    MarkAs { mark_as: StatusAfterProcessing },
    #[serde(untagged)]
    MoveAndPrune { move_to_and_prune: String },
}

#[derive(Deserialize, Debug, PartialEq, Eq, Copy, Clone)]
pub(super) enum StatusAfterProcessing {
    Done,
    ToPrune,
}

impl AfterProcessing {
    pub(super) async fn run(
        &self,
        spec: &FileSpec,
        config: &Config,
        db: &Database,
    ) -> Option<ProcessStatus> {
        match self {
            AfterProcessing::Pass => None,
            AfterProcessing::MarkAs { mark_as } => match mark_as {
                StatusAfterProcessing::Done => Some(ProcessStatus::Done),
                StatusAfterProcessing::ToPrune => Some(ProcessStatus::ToPrune),
            },
            AfterProcessing::MoveAndPrune { move_to_and_prune } => {
                let rep = Replacements::new(spec, config);
                let dest = rep.apply_to(move_to_and_prune);
                match fs::rename(&rep.server_path, &dest) {
                    Ok(()) => match db.remove(spec.hash()).await {
                        Ok(()) => None,
                        Err(err) => {
                            warn!("error when removing {spec:?} from db: {err}");
                            Some(ProcessStatus::ToPrune)
                        }
                    },
                    Err(err) if err.kind() == io::ErrorKind::CrossesDevices => {
                        match fs::copy(&rep.server_path, &dest) {
                            Ok(_) => Some(ProcessStatus::ToPrune),
                            Err(err) => {
                                warn!("failed copying {spec:?}: {err:?}");
                                Some(ProcessStatus::Failed)
                            }
                        }
                    }
                    Err(err) => {
                        warn!("failed moving {spec:?}: {err:?}");
                        Some(ProcessStatus::Failed)
                    }
                }
            }
        }
    }
}

impl Processing {
    pub(super) async fn run(&self, file: &FileSpec, config: &Config) -> io::Result<()> {
        match &self.0 {
            InnerProc::One(step) => {
                let rep = Replacements::new(file, config);
                step.run(&rep).await
            }
            InnerProc::List(steps) => {
                let rep = Replacements::new(file, config);
                for step in steps {
                    step.run(&rep).await?;
                }
                Ok(())
            }
            InnerProc::Pass => Ok(()),
        }
    }
}
