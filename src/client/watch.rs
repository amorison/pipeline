use std::{
    collections::HashSet,
    io,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use futures_util::SinkExt;
use log::{debug, info};
use tokio::{
    io::AsyncWrite,
    net::tcp::OwnedWriteHalf,
    sync::{Mutex, Semaphore},
    task::yield_now,
    time::Instant,
};
use walkdir::{DirEntry, WalkDir};

use crate::{
    FileInfo, FileSpec,
    client::{Config, Db, ToServer},
    framed_io::framed_json_sink,
};

async fn insert_path(db: &Db, path: &Path) -> bool {
    let mut db = db.lock().await;
    if db.contains(path) {
        false
    } else {
        db.insert(path.to_owned())
    }
}

async fn file_info_if_new(
    root: &Path,
    entry: &DirEntry,
    db: &Db,
    conf: &Config,
) -> io::Result<Option<FileInfo>> {
    for group in &conf.watching.groups {
        if group.validate(entry)? {
            let relative_path = entry
                .path()
                .strip_prefix(root)
                .expect("root should be parent of path");

            // Extra sanitation: check that the file name and relative
            // path can be represented as UTF8 strings, to safely send
            // over the network.
            let Some(filename) = entry.file_name().to_str() else {
                return Ok(None);
            };

            let segments: Option<Vec<&str>> = relative_path
                .parent()
                .unwrap()
                .iter()
                .map(|segment| segment.to_str())
                .collect();
            let Some(segments) = segments else {
                return Ok(None);
            };

            if insert_path(db, relative_path).await {
                let info = FileInfo {
                    filename: filename.to_owned(),
                    relpath: segments.join("/"),
                    processing: group.processing.clone(),
                    full_hash: group.full_hash,
                };
                return Ok(Some(info));
            }
            return Ok(None);
        }
    }
    Ok(None)
}

async fn examine_file<W: AsyncWrite + Unpin>(
    root: PathBuf,
    entry: DirEntry,
    to_server: ToServer<W>,
    db: Db,
    conf: Arc<Config>,
    semaphore: Arc<Semaphore>,
) -> io::Result<bool> {
    debug!("examining {:?}", entry.path());
    if let Ok(Some(info)) = file_info_if_new(&root, &entry, &db, &conf).await
        && let Ok(spec) = {
            let permit = semaphore.acquire_owned().await.unwrap();
            tokio::task::spawn_blocking(move || {
                let spec = FileSpec::new(conf.name.clone(), entry.path(), info);
                drop(permit);
                spec
            })
            .await
            .unwrap()
        }
    {
        debug!("found file to process {spec:?}");
        to_server.lock().await.send(spec).await?;
        Ok(true)
    } else {
        Ok(false)
    }
}

async fn recurse_through_files<W: AsyncWrite + Unpin + Send + 'static>(
    root: PathBuf,
    to_server: ToServer<W>,
    db: Db,
    conf: Arc<Config>,
) -> io::Result<u64> {
    let mut examined_files = Vec::with_capacity(32);
    let semaphore = Arc::new(Semaphore::new(conf.watching.max_concurrent_hashes));
    let mut found_files = 0;
    let walker = WalkDir::new(&root)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file());
    for entry in walker {
        let root = root.clone();
        let to_server = to_server.clone();
        let db = db.clone();
        let conf = conf.clone();
        let semaphore = semaphore.clone();
        examined_files.push(tokio::spawn(async move {
            examine_file(root, entry, to_server, db, conf, semaphore).await
        }));
        yield_now().await;
    }
    for f in examined_files {
        if f.await?? {
            found_files += 1;
        }
    }
    Ok(found_files)
}

struct HeartBeat {
    nfiles: u64,
    nrefreshes: u32,
    emit_every_refreshes: u32,
    timer: Instant,
}

impl HeartBeat {
    fn new(emit_every_refreshes: u32) -> Self {
        Self {
            nfiles: 0,
            nrefreshes: 0,
            emit_every_refreshes,
            timer: Instant::now(),
        }
    }

    fn refresh(&mut self, n_new_files: u64) {
        self.nfiles += n_new_files;
        if self.emit_every_refreshes > 0 {
            self.nrefreshes = (self.nrefreshes + 1) % self.emit_every_refreshes;
            if self.nrefreshes == 0 {
                self.emit();
            }
        }
    }

    fn emit(&mut self) {
        let elapsed = self.timer.elapsed();
        self.timer = Instant::now();
        info!(
            "found {} new files to process since last heartbeat ({:.0} s ago)",
            self.nfiles,
            elapsed.as_secs_f64(),
        );
        self.nfiles = 0;
    }
}

pub(super) async fn watch_dir(
    to_server: ToServer<OwnedWriteHalf>,
    db: Db,
    conf: Arc<Config>,
    once: bool,
) -> io::Result<()> {
    info!("watching {:?} for new files", &conf.watching.directory);
    let mut interval = tokio::time::interval(Duration::from_secs(conf.watching.refresh_every_secs));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let root = conf.watching.directory.canonicalize()?;
    let mut heart_beat = HeartBeat::new(conf.watching.heartbeat_every_refreshes);
    loop {
        interval.tick().await;
        debug!("going through files in {root:?}");
        let nfiles =
            recurse_through_files(root.clone(), to_server.clone(), db.clone(), conf.clone())
                .await?;
        heart_beat.refresh(nfiles);
        if once && nfiles == 0 && db.lock().await.is_empty() {
            heart_beat.emit();
            info!("stopping as in `start-once` mode and no new file has been found");
            break Ok(());
        }
    }
}

pub(crate) async fn main(config: Config) -> io::Result<()> {
    let config = Arc::new(config);
    let db = Arc::new(Mutex::new(HashSet::new()));
    let root = config.watching.directory.canonicalize()?;
    let to_server = framed_json_sink();
    let to_server = Arc::new(Mutex::new(to_server));
    let timer = Instant::now();
    recurse_through_files(root, to_server, db.clone(), config.clone()).await?;
    let duration = timer.elapsed();
    println!(
        "watched-files: found {} files to process in {:?}, took {:.3} s",
        db.lock().await.len(),
        config.watching.directory,
        duration.as_secs_f64(),
    );
    Ok(())
}
