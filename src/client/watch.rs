use std::{ffi::OsStr, io, path::Path, sync::Arc, time::Duration};

use futures_util::SinkExt;
use log::{debug, info};
use tokio::{fs, sync::Mutex};

use crate::{
    FileSpec, WriteFramedJson,
    client::{Config, Db},
};

fn insert_path(db: &Db, path: &Path) -> bool {
    let mut db = db.try_lock().unwrap();
    if db.contains(path) {
        false
    } else {
        db.insert(path.to_owned())
    }
}

fn is_new_watched_path(root: &Path, path: &Path, db: &Db, conf: &Config) -> io::Result<bool> {
    if path
        .extension()
        .is_some_and(|ext| *ext == *conf.watching.extension)
        && path.file_name().map(OsStr::to_str).is_some()
        && let Ok(last_modif) = path.metadata()?.modified()?.elapsed()
        && last_modif > Duration::from_secs(conf.watching.last_modif_secs)
    {
        Ok(insert_path(db, path.strip_prefix(root).unwrap()))
    } else {
        Ok(false)
    }
}

async fn examine_file(
    root: &Path,
    path: &Path,
    to_server: Arc<Mutex<WriteFramedJson<FileSpec>>>,
    db: &Db,
    conf: &Config,
) -> io::Result<()> {
    debug!("examining {path:?}");
    if let Ok(true) = is_new_watched_path(root, path, db, conf)
        && let Ok(spec) = FileSpec::new(conf.name.clone(), root, path)
    {
        info!("found file to process {spec:?}");
        to_server.lock().await.send(spec).await?;
    }
    Ok(())
}

async fn recurse_through_files(
    root: &Path,
    dir: &Path,
    to_server: Arc<Mutex<WriteFramedJson<FileSpec>>>,
    db: &Db,
    conf: &Config,
) -> io::Result<()> {
    let mut read_dir = fs::read_dir(dir).await?;
    while let Some(entry) = read_dir.next_entry().await? {
        let path = entry.path();
        if path.is_file() {
            examine_file(root, &path, to_server.clone(), db, conf).await?;
        } else if path.is_dir() {
            Box::pin(recurse_through_files(
                root,
                &path,
                to_server.clone(),
                db,
                conf,
            ))
            .await?;
        }
    }
    Ok(())
}

pub(super) async fn watch_dir(
    to_server: Arc<Mutex<WriteFramedJson<FileSpec>>>,
    db: Db,
    conf: Arc<Config>,
) -> io::Result<()> {
    info!(
        "watching {:?} for {} files",
        &conf.watching.directory, conf.watching.extension
    );
    let mut interval = tokio::time::interval(Duration::from_secs(conf.watching.refresh_every_secs));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let root = conf.watching.directory.canonicalize()?;
    loop {
        interval.tick().await;
        debug!("going through files in {root:?}");
        recurse_through_files(&root, &root, to_server.clone(), &db, &conf).await?;
    }
}
