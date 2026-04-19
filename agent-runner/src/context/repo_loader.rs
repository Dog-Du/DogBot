use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct RepoContentLoader {
    pub root: String,
}

impl RepoContentLoader {
    pub fn new(root: impl Into<String>) -> Self {
        Self { root: root.into() }
    }

    pub fn root_path(&self) -> &Path {
        Path::new(&self.root)
    }

    pub fn root_path_buf(&self) -> PathBuf {
        PathBuf::from(&self.root)
    }
}
