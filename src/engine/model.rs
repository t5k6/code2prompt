// src/model.rs

//! Contains the core data structures for the application.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::SystemTime;

// --- Moved from token_map.rs ---

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
pub struct EntryMetadata {
    pub is_dir: bool,
    pub is_symlink: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct TreeNode {
    pub(crate) tokens: usize,
    pub(crate) children: BTreeMap<String, TreeNode>,
    pub(crate) path: String,
    pub(crate) metadata: Option<EntryMetadata>,
}

impl TreeNode {
    pub(crate) fn with_path(path: String) -> Self {
        TreeNode {
            tokens: 0,
            children: BTreeMap::new(),
            path,
            metadata: None,
        }
    }
}

#[derive(Debug)]
pub struct TokenMapEntry {
    pub path: String,
    pub name: String,
    pub tokens: usize,
    pub percentage: f64,
    pub depth: usize,
    pub is_last: bool,
    pub metadata: EntryMetadata,
}

/// Holds all relevant information about a processed file.
#[derive(Debug, Clone)]
pub struct ProcessedEntry {
    pub path: PathBuf,
    pub relative_path: PathBuf,
    pub is_file: bool,
    pub code: Option<String>,
    pub extension: Option<String>,
    pub token_count: Option<usize>,
    pub mtime: Option<SystemTime>,
}
