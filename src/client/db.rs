use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::Arc,
};

use tokio::sync::Mutex;

#[derive(Clone)]
pub(super) struct Db {
    found_files: Arc<Mutex<HashSet<PathBuf>>>,
    to_keep: Arc<Mutex<HashSet<PathBuf>>>,
}

impl Db {
    pub(super) fn new() -> Self {
        Self {
            found_files: Arc::new(Mutex::new(HashSet::new())),
            to_keep: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    pub(super) async fn n_found_files(&self) -> usize {
        self.found_files.lock().await.len()
    }

    pub(super) async fn has_found_files(&self) -> bool {
        !self.found_files.lock().await.is_empty()
    }

    pub(super) async fn is_to_keep(&self, path: &Path) -> bool {
        self.to_keep.lock().await.contains(path)
    }

    pub(super) async fn received(&self, path: &Path) {
        self.found_files.lock().await.remove(path);
    }

    pub(super) async fn forget(&self, path: &Path) {
        self.found_files.lock().await.remove(path);
        self.to_keep.lock().await.remove(path);
    }

    #[must_use]
    pub(super) fn start_watch_loop(self) -> DbInWatchLoop {
        DbInWatchLoop {
            db: self,
            seen_files: Arc::new(Mutex::new(HashSet::new())),
        }
    }
}

#[derive(Clone)]
pub(super) struct DbInWatchLoop {
    db: Db,
    seen_files: Arc<Mutex<HashSet<PathBuf>>>,
}

impl DbInWatchLoop {
    pub(super) async fn insert_path(&self, path: &Path, keep: bool) -> bool {
        self.seen_files.lock().await.insert(path.to_owned());
        let mut ff = self.db.found_files.lock().await;
        let mut tk = self.db.to_keep.lock().await;
        if ff.contains(path) || tk.contains(path) {
            false
        } else {
            if keep {
                tk.insert(path.to_owned());
            }
            ff.insert(path.to_owned())
        }
    }

    // This would be better implemented as `AsyncDrop` so the calling code doesn't have to call this
    // explicitly, but `AsyncDrop` is still nightly.
    pub(super) async fn end_watch_loop(self) {
        let seen_files = self.seen_files.lock().await;
        self.db
            .to_keep
            .lock()
            .await
            .retain(|path| seen_files.contains(path));
    }
}
