//! Contains the core data structures for the application.

use std::{collections::BTreeMap, path::PathBuf, time::SystemTime};

use serde::{Deserialize, Serialize};

use crate::ui::tree_arena::PathInfo;

/// The complete, serializable context passed to the template engine.
#[derive(Debug, Serialize)]
pub struct TemplateContext {
    pub absolute_code_path: String,
    pub files: Vec<FileContext>,
    pub source_tree: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_diff: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_diff_branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_log_branch: Option<String>,
}

/// Represents a single file within the template context.
#[derive(Debug, Serialize)]
pub struct FileContext {
    pub path: String,
    pub extension: String,
    pub code: String,
    pub token_count: Option<usize>,
}

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

#[cfg(feature = "tui")]
impl PathInfo for ProcessedEntry {
    fn path(&self) -> &str {
        // Use the relative path for the tree
        self.relative_path.to_str().unwrap_or_default()
    }

    fn count(&self) -> usize {
        // The tree arena uses this to sum up file counts. Each entry is one file.
        1
    }

    fn extension(&self) -> Option<&String> {
        self.extension.as_ref()
    }
    fn token_count(&self) -> Option<usize> {
        self.token_count
    }
}
