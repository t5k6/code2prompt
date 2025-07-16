use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::{
    common::cache::{CacheFormat, Cacheable},
    engine::utils::RepoCachePath,
};

/// Caches the user's last selections in the TUI for a given repository.
#[derive(Serialize, Deserialize, Debug, Default)]
pub struct LastSelection {
    pub extensions: Vec<String>,
    pub directories: Vec<String>,
}

impl Cacheable for LastSelection {
    const KEY: &'static str = "selection";
    const FORMAT: CacheFormat = CacheFormat::Json;
}

fn get_cache_path(repo_path: &Path) -> Result<PathBuf> {
    RepoCachePath::new(repo_path)?.get_cache_file_path("selection", "json")
}

pub fn load_last_selection(repo_path: &Path) -> Result<Option<LastSelection>> {
    let cache_path = get_cache_path(repo_path)?;
    if !cache_path.exists() {
        return Ok(None);
    }
    let file_content = std::fs::read_to_string(cache_path)?;
    let selection: LastSelection = serde_json::from_str(&file_content)?;
    Ok(Some(selection))
}

pub fn save_last_selection(
    repo_path: &Path,
    extensions: &[String],
    directories: &[String],
) -> Result<()> {
    let cache_path = get_cache_path(repo_path)?;
    let selection = LastSelection {
        extensions: extensions.to_vec(),
        directories: directories.to_vec(),
    };
    let json = serde_json::to_string_pretty(&selection)?;
    std::fs::write(cache_path, json)?;
    Ok(())
}
