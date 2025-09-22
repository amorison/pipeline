use std::{
    ffi::OsStr,
    io,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use futures_util::SinkExt;
use log::{debug, info};
use tokio::{
    fs,
    sync::{Mutex, Semaphore},
};

use crate::{
    FileSpec, WriteFramedJson,
    client::{Config, Db},
};

fn insert_path(db: Db, path: &Path) -> bool {
    let mut db = db.try_lock().unwrap();
    if db.contains(path) {
        false
    } else {
        db.insert(path.to_owned())
    }
}

fn is_new_watched_path(root: &Path, path: &Path, db: Db, conf: &Config) -> io::Result<bool> {
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
    root: PathBuf,
    path: PathBuf,
    to_server: Arc<Mutex<WriteFramedJson<FileSpec>>>,
    db: Db,
    conf: Arc<Config>,
    semaphore: Arc<Semaphore>,
) -> io::Result<()> {
    debug!("examining {path:?}");
    if let Ok(true) = is_new_watched_path(&root, &path, db, &conf)
        && let Ok(spec) = {
            let client_name = conf.name.clone();
            let root = root.to_owned();
            let path = path.to_owned();
            let permit = semaphore.acquire_owned().await.unwrap();
            tokio::task::spawn_blocking(move || {
                let spec = FileSpec::new(client_name, &root, &path);
                drop(permit);
                spec
            })
            .await
            .unwrap()
        }
    {
        info!("found file to process {spec:?}");
        to_server.lock().await.send(spec).await?;
    }
    Ok(())
}

async fn recurse_through_files(
    root: PathBuf,
    dir: &Path,
    to_server: Arc<Mutex<WriteFramedJson<FileSpec>>>,
    db: Db,
    conf: Arc<Config>,
) -> io::Result<()> {
    let mut examined_files = Vec::with_capacity(32);
    let mut read_dir = fs::read_dir(dir).await?;
    let semaphore = Arc::new(Semaphore::new(conf.watching.max_concurrent_hashes));
    while let Some(entry) = read_dir.next_entry().await? {
        let path = entry.path();
        if path.is_file() {
            let root = root.clone();
            let to_server = to_server.clone();
            let db = db.clone();
            let conf = conf.clone();
            let semaphore = semaphore.clone();
            examined_files.push(tokio::spawn(async move {
                examine_file(root, path, to_server, db, conf, semaphore).await
            }));
        } else if path.is_dir() {
            Box::pin(recurse_through_files(
                root.clone(),
                &path,
                to_server.clone(),
                db.clone(),
                conf.clone(),
            ))
            .await?;
        }
    }
    for f in examined_files {
        f.await??;
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
        recurse_through_files(
            root.clone(),
            &root,
            to_server.clone(),
            db.clone(),
            conf.clone(),
        )
        .await?;
    }
}
