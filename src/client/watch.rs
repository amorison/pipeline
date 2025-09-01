use std::{ffi::OsStr, io, path::PathBuf, sync::Arc, time::Duration};

use futures_util::SinkExt;
use log::info;
use tokio::{fs, sync::Mutex};

use crate::{
    FileSpec, WriteFramedJson,
    client::{Config, Db},
};

fn insert_clone(db: &Db, path: &PathBuf) -> bool {
    let mut db = db.try_lock().unwrap();
    if db.contains(path) {
        false
    } else {
        db.insert(path.clone())
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
            if entry.file_type().await?.is_file() {
                let client_path = entry.path();
                if client_path
                    .extension()
                    .is_some_and(|ext| *ext == *conf.watching.extension)
                    && client_path.file_name().map(OsStr::to_str).is_some()
                    && let Ok(last_modif) = client_path.metadata()?.modified()?.elapsed()
                    && last_modif > Duration::from_secs(conf.watching.last_modif_secs)
                    && insert_clone(&db, &client_path)
                {
                    let nfp = FileSpec::new(client_path)?;
                    info!("found file to process {nfp:?}");
                    to_server.lock().await.send(nfp).await?;
                }
            }
        }
        tokio::time::sleep(Duration::from_secs(conf.watching.refresh_every_secs)).await;
    }
}
