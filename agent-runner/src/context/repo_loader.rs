use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PackManifest {
    pub pack_id: String,
    pub version: u32,
    pub title: String,
    pub kind: String,
    pub source: PackSource,
    pub items: Vec<PackItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PackSource {
    pub source_id: String,
    pub repo_url: String,
    #[serde(rename = "ref")]
    pub ref_: String,
    pub license: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PackItem {
    pub id: String,
    pub kind: String,
    pub path: String,
    pub title: String,
    pub summary: String,
    pub tags: Vec<String>,
    pub enabled_by_default: bool,
    pub platform_overrides: Vec<String>,
    pub upstream_path: String,
}

#[derive(Debug, Error)]
pub enum RepoLoaderError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

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

    pub fn load_packs(&self) -> Result<Vec<PackManifest>, RepoLoaderError> {
        let packs_dir = self.root_path().join("packs");
        if !packs_dir.exists() {
            return Ok(Vec::new());
        }

        let mut manifests = Vec::new();
        for entry in fs::read_dir(packs_dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }

            let manifest_path = entry.path().join("manifest.json");
            if !manifest_path.is_file() {
                continue;
            }

            let raw = fs::read_to_string(&manifest_path)?;
            let manifest: PackManifest = serde_json::from_str(&raw)?;
            manifests.push(manifest);
        }

        manifests.sort_by(|left, right| left.pack_id.cmp(&right.pack_id));
        Ok(manifests)
    }

    pub fn enabled_items(&self) -> Result<Vec<PackItem>, RepoLoaderError> {
        Ok(self
            .load_packs()?
            .into_iter()
            .flat_map(|pack| pack.items.into_iter())
            .filter(|item| item.enabled_by_default)
            .collect())
    }
}
