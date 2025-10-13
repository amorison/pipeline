use serde::Deserialize;
use tokio::{io, process::Command};

use crate::{FileSpec, replace_os_strings, server::Config};

#[derive(Deserialize, Debug, PartialEq, Eq)]
pub(super) struct Processing(Vec<String>);

impl Processing {
    pub(super) async fn run(&self, file: &FileSpec, config: &Config) -> io::Result<()> {
        let server_path = config.path_of(file);

        let mut processing = Command::new(&self.0[0])
            .args(self.0[1..].iter().map(|a| {
                replace_os_strings(
                    a,
                    [
                        ("{hash}", file.hash().as_ref()),
                        ("{server_path}", server_path.as_os_str()),
                        ("{client_name}", file.client.as_ref()),
                        (
                            "{client_relative_directory}",
                            file.relative_directory().as_os_str(),
                        ),
                        ("{client_file_stem}", file.file_stem()),
                    ]
                    .into_iter(),
                )
            }))
            .spawn()?;

        match processing.wait().await {
            Ok(status) if status.success() => Ok(()),
            Ok(status) => Err(io::Error::other(format!("failed with status {status:?}"))),
            Err(err) => Err(err),
        }
    }
}
