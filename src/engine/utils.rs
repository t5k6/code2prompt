use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};

#[derive(Clone)]
pub struct RepoCachePath {
    repo_hash: String,
}

impl RepoCachePath {
    pub fn new<P: AsRef<Path>>(repo_root: P) -> Result<Self> {
        let repo_root = repo_root.as_ref();
        if !repo_root.exists() {
            return Err(anyhow::anyhow!(
                "Repository path does not exist: {}",
                repo_root.display()
            ));
        }

        let canonical_path = repo_root.canonicalize().with_context(|| {
            format!(
                "Failed to canonicalize repository path: {}",
                repo_root.display()
            )
        })?;

        let canonical_path_string = canonical_path.to_string_lossy();
        let repo_hash = {
            let hash = Sha256::digest(canonical_path_string.as_bytes());
            hex::encode(hash)
        };

        Ok(Self { repo_hash })
    }

    pub fn get_cache_file_path(&self, prefix: &str, extension: &str) -> Result<PathBuf> {
        let cache_dir = dirs::cache_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("code2prompt");
        std::fs::create_dir_all(&cache_dir).context("Failed to create cache directory")?;

        Ok(cache_dir.join(format!("{}_{}.{}", prefix, self.repo_hash, extension)))
    }
}
