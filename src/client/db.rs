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

    pub(super) async fn insert_path(&self, path: &Path, keep: bool) -> bool {
        let mut ff = self.found_files.lock().await;
        let mut tk = self.to_keep.lock().await;
        if ff.contains(path) || tk.contains(path) {
            false
        } else {
            if keep {
                tk.insert(path.to_owned());
            }
            ff.insert(path.to_owned())
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
}
