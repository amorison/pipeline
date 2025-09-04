use std::{ffi::OsStr, io, path::Path, sync::Arc, time::Duration};

use futures_util::SinkExt;
use log::info;
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

fn is_new_watched_path(path: &Path, db: &Db, conf: &Config) -> io::Result<bool> {
    if path
        .extension()
        .is_some_and(|ext| *ext == *conf.watching.extension)
        && path.file_name().map(OsStr::to_str).is_some()
        && let Ok(last_modif) = path.metadata()?.modified()?.elapsed()
        && last_modif > Duration::from_secs(conf.watching.last_modif_secs)
    {
        Ok(insert_path(db, path))
    } else {
        Ok(false)
    }
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
    loop {
        let mut files = fs::read_dir(&conf.watching.directory).await?;
        while let Some(entry) = files.next_entry().await? {
            if let Ok(ft) = entry.file_type().await
                && ft.is_file()
            {
                let client_path = entry.path();
                if let Ok(true) = is_new_watched_path(&client_path, &db, &conf)
                    && let Ok(spec) = FileSpec::new(&client_path)
                {
                    info!("found file to process {spec:?}");
                    to_server.lock().await.send(spec).await?;
                }
            }
        }
        tokio::time::sleep(Duration::from_secs(conf.watching.refresh_every_secs)).await;
    }
}
