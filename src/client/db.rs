use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::Arc,
};

use tokio::sync::Mutex;

#[derive(Clone)]
pub(super) struct Db {
    found_files: Arc<Mutex<HashSet<PathBuf>>>,
}

impl Db {
    pub(super) fn new() -> Self {
        Self {
            found_files: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    pub(super) async fn insert_path(&self, path: &Path) -> bool {
        let mut db = self.found_files.lock().await;
        if db.contains(path) {
            false
        } else {
            db.insert(path.to_owned())
        }
    }

    pub(super) async fn n_found_files(&self) -> usize {
        self.found_files.lock().await.len()
    }

    pub(super) async fn has_found_files(&self) -> bool {
        !self.found_files.lock().await.is_empty()
    }

    pub(super) async fn remove(&self, path: &Path) {
        self.found_files.lock().await.remove(path);
    }
}
