use std::{fmt::Display, fs::Metadata, io, sync::Arc};

use log::{debug, warn};

use crate::{
    FileSpec,
    server::{
        Config,
        database::{Database, ProcessStatus},
    },
};

fn format_size(size: u64) -> String {
    const GIBI: u64 = 1024u64.pow(3);
    const MEBI: u64 = 1024u64.pow(2);
    const KIBI: u64 = 1024u64.pow(1);
    let (size, unit) = if size > GIBI {
        (size as f64 / GIBI as f64, "GiB")
    } else if size > MEBI {
        (size as f64 / MEBI as f64, "MiB")
    } else if size > KIBI {
        (size as f64 / KIBI as f64, "kiB")
    } else {
        (size as f64, "B")
    };
    if unit == "B" {
        format!("{size:.0} {unit}")
    } else {
        format!("{size:.1} {unit}")
    }
}

pub(super) struct CleanSummary {
    nfiles: u32,
    total_size: u64,
}

impl CleanSummary {
    fn new() -> CleanSummary {
        CleanSummary {
            nfiles: 0,
            total_size: 0,
        }
    }

    fn add(&mut self, meta: Metadata) {
        self.nfiles += 1;
        self.total_size += meta.len();
    }
}

impl Display for CleanSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let fmt_size = format_size(self.total_size);
        write!(f, "deleted {} files ({fmt_size})", self.nfiles)
    }
}

pub(super) async fn clean_tasks_with_status(
    config: Arc<Config>,
    db: Database,
    status: ProcessStatus,
) -> CleanSummary {
    debug!("looking for tasks to prune");
    let mut summary = CleanSummary::new();
    let to_prune = db.tasks_with_status(status).await;
    match to_prune {
        Ok(to_prune) => {
            for spec in to_prune.into_iter().map(FileSpec::from) {
                debug!("pruning {spec:?}");
                let server_path = config.path_of(&spec);
                match tokio::fs::metadata(&server_path).await {
                    Ok(meta) => summary.add(meta),
                    Err(err) => warn!("error gathering metadata for {spec:?}: {err}"),
                }
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
    summary
}

pub(crate) async fn main(config: Config, include_done: bool) -> io::Result<()> {
    let db = Database::create_if_missing()
        .await
        .expect("failed to create database");

    let config = Arc::new(config);

    if include_done {
        let summary =
            clean_tasks_with_status(config.clone(), db.clone(), ProcessStatus::Done).await;
        println!("Done files: {summary}")
    }

    let summary = clean_tasks_with_status(config, db, ProcessStatus::ToPrune).await;
    println!("ToPrune files: {summary}");

    Ok(())
}
