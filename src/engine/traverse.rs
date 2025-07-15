//! This module contains the functions for traversing the directory and processing the files.

use crate::engine::config::Code2PromptConfig;
use crate::engine::filter::should_include_file;
use crate::engine::model::ProcessedEntry;
use crate::engine::token::count_tokens;
use anyhow::{bail, Context, Result};
use dashmap::{DashMap, DashSet};
use globset::GlobSetBuilder;
use ignore::{WalkBuilder, WalkState};
use log::warn;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;
use termtree::Tree;

const MAX_FILE_SIZE_BYTES: u64 = 1_048_576; // 1 MiB

/// Defines the behavior of the `process_codebase` function.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessingMode {
    /// Reads file content, counts tokens, and prepares for prompt generation.
    FullProcess,
    /// Only stats files to collect their extensions. Does not read content.
    ExtensionCollection,
}
/// Traverses the directory, processes files in parallel, and collects necessary data.
pub fn process_codebase(
    config: &Code2PromptConfig,
    mode: ProcessingMode,
) -> Result<(
    Vec<ProcessedEntry>,
    HashSet<String>,
    HashMap<String, usize>, // MODIFIED: Return directory counts
)> {
    let mut include_builder = GlobSetBuilder::new();
    for p in &config.include_patterns {
        include_builder.add(globset::Glob::new(p.as_str())?);
    }
    let include_set = include_builder.build()?;

    let mut exclude_builder = GlobSetBuilder::new();
    for p in &config.exclude_patterns {
        exclude_builder.add(globset::Glob::new(p.as_str())?);
    }
    let exclude_set = exclude_builder.build()?;

    let canonical_root = config.path.canonicalize().with_context(|| {
        format!(
            "Failed to canonicalize root path: {}",
            config.path.display()
        )
    })?;

    // These thread-safe collectors will hold the final, aggregated results.
    let processed_entries_agg = Arc::new(Mutex::new(Vec::new()));
    let extensions_agg = Arc::new(DashSet::new());
    // NEW: Add a thread-safe map for directory file counts
    let dirs_agg = Arc::new(DashMap::<PathBuf, usize>::new());

    let mut walker_builder = WalkBuilder::new(&canonical_root);
    walker_builder
        .follow_links(config.follow_symlinks)
        .hidden(!config.hidden)
        .git_ignore(!config.no_ignore);

    let walker = walker_builder.build_parallel();
    let config_arc = Arc::new(config.clone());
    let canonical_root_arc = Arc::new(canonical_root);

    // Helper struct to collect per-thread results and merge them on drop.
    // This ensures we only lock the main mutex once per thread.
    struct ThreadResultCollector<'a> {
        local_entries: Vec<ProcessedEntry>,
        aggregator: Arc<Mutex<Vec<ProcessedEntry>>>,
        // We don't need to collect extensions this way because DashSet is highly concurrent.
        phantom: std::marker::PhantomData<&'a ()>,
    }

    impl Drop for ThreadResultCollector<'_> {
        fn drop(&mut self) {
            if !self.local_entries.is_empty() {
                let mut guard = self.aggregator.lock().unwrap();
                guard.append(&mut self.local_entries);
            }
        }
    }

    walker.run(|| {
        let include_set = include_set.clone();
        let exclude_set = exclude_set.clone();
        let canonical_root = canonical_root_arc.clone();

        let config = config_arc.clone();
        let extensions = extensions_agg.clone();
        // NEW: Clone the directory aggregator for the thread
        let dirs = dirs_agg.clone();

        // Each thread gets its own collector.
        let mut collector = ThreadResultCollector {
            local_entries: Vec::new(),
            aggregator: processed_entries_agg.clone(),
            phantom: std::marker::PhantomData,
        };

        // The inner closure can now move the cheap clones.
        Box::new(move |entry_result| {
            let entry = match entry_result {
                Ok(e) => e,
                Err(e) => {
                    warn!("Skipping entry due to error: {}", e);
                    return WalkState::Continue;
                }
            };

            if !should_include_file(
                entry.path(),
                &canonical_root, // Use the cloned canonical_root
                &include_set,    // Use the cloned include_set
                &exclude_set,    // Use the cloned exclude_set
                config.include_priority,
            ) {
                return WalkState::Continue;
            }

            if !entry.file_type().is_some_and(|ft| ft.is_file()) {
                return WalkState::Continue;
            }

            let path = entry.path();
            if let Some(ext) = path.extension().and_then(|ext| ext.to_str()) {
                if !ext.is_empty() {
                    extensions.insert(ext.to_string());
                }
            }

            // NEW: Collect parent directories and file-counts
            if let Some(parent) = path.parent() {
                if parent != &*canonical_root {
                    if let Ok(rel_parent) = parent.strip_prefix(&*canonical_root) {
                        if !rel_parent.as_os_str().is_empty() {
                            dirs.entry(rel_parent.to_path_buf())
                                .and_modify(|c| *c += 1)
                                .or_insert(1);
                        }
                    }
                }
            }

            if mode == ProcessingMode::FullProcess {
                let mtime = entry.metadata().ok().and_then(|m| m.modified().ok());
                // The `if let Ok(...)` already handles the `bail!` from the validation checks.
                // No changes are needed here.
                if let Ok(processed) = process_single_file(path, &canonical_root, &config, mtime) {
                    collector.local_entries.push(processed);
                }
            }

            WalkState::Continue
        })
    });

    let final_entries = Arc::try_unwrap(processed_entries_agg)
        .unwrap()
        .into_inner()
        .unwrap();
    let final_extensions = Arc::try_unwrap(extensions_agg)
        .unwrap()
        .into_iter()
        .collect();

    // NEW: Collect final directory counts
    let final_dirs: HashMap<String, usize> = Arc::try_unwrap(dirs_agg)
        .unwrap()
        .into_iter()
        .map(|(path_buf, count)| (path_buf.to_string_lossy().replace('\\', "/"), count))
        .collect();

    Ok((final_entries, final_extensions, final_dirs))
}

fn process_single_file(
    path: &Path,
    root: &Path,
    config: &Code2PromptConfig,
    mtime: Option<SystemTime>,
) -> Result<ProcessedEntry> {
    let metadata = fs::metadata(path)
        .with_context(|| format!("Failed to read metadata for {}", path.display()))?;

    let file_size = metadata.len();

    if file_size == 0 {
        // No need to log here, empty files are common and not usually an error.
        // We just skip them by returning an error that the calling loop will ignore.
        bail!("File is empty");
    }

    if file_size > MAX_FILE_SIZE_BYTES {
        warn!(
            "Skipping oversized file ({} > {} bytes): {}",
            file_size,
            MAX_FILE_SIZE_BYTES,
            path.display()
        );
        bail!("File exceeds size limit");
    }

    let code = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(_) => {
            warn!("Skipping non-UTF-8 file: {}", path.display());
            bail!("File is not valid UTF-8");
        }
    };

    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(String::from);

    let token_count = if config.token_map_enabled {
        count_tokens(&code, config.tokenizer).ok()
    } else {
        None
    };

    let code_block = wrap_code_block(
        &code,
        extension.as_deref().unwrap_or(""),
        config.line_numbers,
        config.no_codeblock,
    );

    let relative_path = path.strip_prefix(root).unwrap_or(path).to_path_buf();

    Ok(ProcessedEntry {
        path: path.to_path_buf(),
        relative_path,
        is_file: true,
        code: Some(code_block),
        extension,
        token_count,
        mtime,
    })
}

pub fn rebuild_tree(
    root_path: &Path,
    entries: &[ProcessedEntry],
    full_directory_tree: bool,
) -> String {
    let canonical_root = root_path
        .canonicalize()
        .unwrap_or_else(|_| root_path.to_path_buf());
    let mut root_tree = Tree::new(label(&canonical_root));

    if !full_directory_tree {
        let mut leaves: Vec<_> = entries
            .iter()
            .map(|entry| Tree::new(entry.relative_path.to_string_lossy().to_string()))
            .collect();
        leaves.sort_by(|a, b| a.root.cmp(&b.root));
        root_tree.leaves = leaves;
        return root_tree.to_string();
    }

    let mut sorted_entries = entries.to_vec();
    sorted_entries.sort_by_key(|e| e.path.clone());

    for entry in &sorted_entries {
        if let Ok(relative_path) = entry.path.strip_prefix(&canonical_root) {
            let mut current_tree = &mut root_tree;
            for component in relative_path.components() {
                let component_str = component.as_os_str().to_string_lossy().to_string();

                current_tree = if let Some(pos) = current_tree
                    .leaves
                    .iter_mut()
                    .position(|child| child.root == component_str)
                {
                    &mut current_tree.leaves[pos]
                } else {
                    let new_tree = Tree::new(component_str);
                    current_tree.leaves.push(new_tree);
                    current_tree.leaves.last_mut().unwrap()
                };
            }
        }
    }

    root_tree.to_string()
}

pub fn label<P: AsRef<Path>>(p: P) -> String {
    let path = p.as_ref();
    if path.file_name().is_none() {
        let current_dir = std::env::current_dir().unwrap();
        current_dir
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(".")
            .to_owned()
    } else {
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("")
            .to_owned()
    }
}

fn wrap_code_block(code: &str, extension: &str, line_numbers: bool, no_codeblock: bool) -> String {
    let delimiter = "`".repeat(3);
    let mut code_with_line_numbers = String::new();

    if line_numbers {
        for (line_number, line) in code.lines().enumerate() {
            code_with_line_numbers.push_str(&format!("{:4} | {}\n", line_number + 1, line));
        }
    } else {
        code_with_line_numbers = code.to_string();
    }

    if no_codeblock {
        code_with_line_numbers
    } else {
        format!(
            "{}{}\n{}\n{}",
            delimiter, extension, code_with_line_numbers, delimiter
        )
    }
}
