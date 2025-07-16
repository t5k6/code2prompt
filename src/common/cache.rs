//! A centralized module for managing file-based caches.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Serialize, de::DeserializeOwned};

use crate::engine::utils::RepoCachePath;

/// The format used for serializing a cache file.
pub enum CacheFormat {
    Json,
    Toml,
}

/// A trait for types that can be saved to and loaded from a cache file.
pub trait Cacheable: Serialize + DeserializeOwned {
    /// A unique key for this cache type, used as the file prefix.
    const KEY: &'static str;
    /// The serialization format to use for the cache file.
    const FORMAT: CacheFormat;
}

/// Manages loading and saving `Cacheable` data types.
pub struct CacheManager {
    /// The base directory for the repository, used to generate a unique hash.
    repo_path_handler: RepoCachePath,
}

impl CacheManager {
    /// Creates a new cache manager for a given repository path.
    pub fn new(repo_path: &Path) -> Result<Self> {
        Ok(Self {
            repo_path_handler: RepoCachePath::new(repo_path)?,
        })
    }

    /// Gets the full, unique path for a given cache file.
    fn get_path_for(&self, key: &str, extension: &str) -> Result<PathBuf> {
        self.repo_path_handler.get_cache_file_path(key, extension)
    }

    /// Saves a `Cacheable` item to its corresponding file.
    pub fn save<T: Cacheable>(&self, item: &T) -> Result<()> {
        let (ext, content) = match T::FORMAT {
            CacheFormat::Json => ("json", serde_json::to_string_pretty(item)?),
            CacheFormat::Toml => ("toml", toml::to_string_pretty(item)?),
        };
        let path = self.get_path_for(T::KEY, ext)?;
        std::fs::create_dir_all(
            path.parent()
                .context("Cache path has no parent directory")?,
        )?;
        std::fs::write(&path, content)
            .with_context(|| format!("Failed to write cache file to {}", path.display()))?;
        Ok(())
    }

    /// Loads a `Cacheable` item from its file, if it exists.
    pub fn load<T: Cacheable>(&self) -> Result<Option<T>> {
        let ext = match T::FORMAT {
            CacheFormat::Json => "json",
            CacheFormat::Toml => "toml",
        };
        let path = self.get_path_for(T::KEY, ext)?;

        if !path.exists() {
            return Ok(None);
        }

        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read cache file from {}", path.display()))?;

        let item = match T::FORMAT {
            CacheFormat::Json => serde_json::from_str(&content)?,
            CacheFormat::Toml => toml::from_str(&content)?,
        };

        Ok(Some(item))
    }
}
