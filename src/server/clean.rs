use std::io;

use log::debug;

use crate::{
    FileSpec,
    server::{
        Config,
        database::{Database, ProcessStatus},
    },
};

pub(crate) async fn main(config: Config, force: bool) -> io::Result<()> {
    let db = Database::create_if_missing()
        .await
        .expect("failed to create database");

    if !force {
        println!("this is a dry run, use the `-f | --force` argument to actually remove files");
    }

    let completed = db
        .tasks_with_status(ProcessStatus::Done)
        .await
        .expect("failed to read database for completed tasks");

    for file in completed.into_iter().map(FileSpec::from) {
        if force {
            println!("removing {file:?}");
            let server_path = config.path_of(&file);
            if let Err(err) = std::fs::remove_file(&server_path) {
                eprintln!("couldn't remove {file:?}: {err}");
            }
            if let Err(err) = db.remove(file.hash()).await {
                debug!("error when removing {file:?} from db: {err}");
            }
        } else {
            println!("would remove {file:?}");
        }
    }
    Ok(())
}
