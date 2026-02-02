use std::{io, sync::Arc};

use log::{debug, warn};

use crate::{
    FileSpec,
    server::{
        Config,
        database::{Database, ProcessStatus},
    },
};

pub(super) async fn clean_tasks_with_status(
    config: Arc<Config>,
    db: Database,
    status: ProcessStatus,
) {
    debug!("looking for tasks to prune");
    let to_prune = db.tasks_with_status(status).await;
    match to_prune {
        Ok(to_prune) => {
            for spec in to_prune.into_iter().map(FileSpec::from) {
                debug!("pruning {spec:?}");
                let server_path = config.path_of(&spec);
                if let Err(err) = tokio::fs::remove_file(&server_path).await {
                    warn!("error pruning {spec:?}: {err}")
                }
                if let Err(err) = db.remove(spec.hash()).await {
                    warn!("error when removing {spec:?} from db: {err}")
                }
            }
        }
        Err(_) => todo!(),
    }
}

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
