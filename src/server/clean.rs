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
        Err(err) => warn!("error when querying db: {err}"),
    }
}

pub(crate) async fn main(config: Config, include_done: bool) -> io::Result<()> {
    let db = Database::create_if_missing()
        .await
        .expect("failed to create database");

    let config = Arc::new(config);

    if include_done {
        clean_tasks_with_status(config.clone(), db.clone(), ProcessStatus::Done).await;
    }

    clean_tasks_with_status(config, db, ProcessStatus::ToPrune).await;

    Ok(())
}
