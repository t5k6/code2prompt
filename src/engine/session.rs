// crates/code2prompt_core/src/session.rs

use crate::engine::config::{Code2PromptConfig, Code2PromptConfigBuilder};
#[cfg(feature = "git")]
use crate::engine::git::{get_git_diff, get_git_diff_between_branches, get_git_log};
use crate::engine::model::{EntryMetadata, ProcessedEntry};
use crate::engine::traverse::{process_codebase, rebuild_tree, ProcessingMode};
use anyhow::Result;
use handlebars::Handlebars;
use serde_json::json;
use std::collections::{HashMap, HashSet}; 

/// Represents a session for generating a prompt from a codebase.
///
/// This struct holds the configuration and the processed data, providing
/// a stateful API to build the final prompt.
#[derive(Debug)]
pub struct Code2PromptSession {
    pub config: Code2PromptConfig,
    pub processed_entries: Vec<ProcessedEntry>,
    pub all_extensions: HashSet<String>,
    pub all_directories: HashMap<String, usize>,
}

impl Code2PromptSession {
    pub fn new(config: Code2PromptConfig) -> Result<Self> {
        // REMOVED all the GlobSet building logic
        Ok(Self {
            config,
            processed_entries: Vec::new(),
            all_extensions: HashSet::new(),
            all_directories: HashMap::new(),
        })
    }

    /// Creates a new session from a config builder.
    pub fn from_builder(builder: Code2PromptConfigBuilder) -> anyhow::Result<Self> {
        // This now returns a Result<Result<Self>>, so we need to flatten it.
        // Or more simply, let the caller handle it.
        // Let's assume the build() itself is the main error source.
        let config = builder.build()?;
        let session = Self::new(config)?;

        Ok(session)
    }

    /// Scans the codebase to collect all file extensions.
    /// Does not read file contents.
    pub fn scan_extensions(&mut self) -> Result<()> {
        let (_, extensions, dirs) = process_codebase(&self.config, ProcessingMode::ExtensionCollection)?;
        self.all_extensions = extensions;
        self.all_directories = dirs;
        Ok(())
    }

    /// Processes the codebase according to the configuration.
    /// This reads file contents and populates the session with data.
    pub fn process_codebase(&mut self) -> Result<()> {
        let (entries, _, _) = process_codebase(&self.config, ProcessingMode::FullProcess)?;
        self.processed_entries = entries;
        Ok(())
    }

    /// Sorts the processed files based on the method specified in the config.
    pub fn sort_files(&mut self) {
        if let Some(sort_method) = &self.config.sort {
            use crate::ui::cli::FileSortMethod;
            match sort_method {
                FileSortMethod::NameAsc => {
                    self.processed_entries.sort_by(|a, b| a.path.cmp(&b.path))
                }
                FileSortMethod::NameDesc => {
                    self.processed_entries.sort_by(|a, b| b.path.cmp(&a.path))
                }
                FileSortMethod::DateAsc => self.processed_entries.sort_by_key(|e| e.mtime),
                FileSortMethod::DateDesc => self
                    .processed_entries
                    .sort_by_key(|e| std::cmp::Reverse(e.mtime)),
            }
        }
    }

    /// Builds the JSON data model required for rendering the template.
    ///
    /// This method gathers all processed file data, the directory tree,
    /// and any git information into a single `serde_json::Value`.
    pub fn build_template_data(
        &self,
        git_diff_args: Option<&str>,
        git_diff_branch_args: Option<(&str, &str)>,
        git_log_branch_args: Option<(&str, &str)>,
    ) -> Result<serde_json::Value> {
        let tree = rebuild_tree(
            &self.config.path,
            &self.processed_entries,
            self.config.full_directory_tree,
        );

        let files_json: Vec<serde_json::Value> = self
            .processed_entries
            .iter()
            .filter_map(|entry| {
                if !entry.is_file {
                    return None;
                }
                let mut file_obj = serde_json::Map::new();
                let path_str = if !self.config.absolute_path {
                    entry.relative_path.to_string_lossy().into_owned()
                } else {
                    entry.path.to_string_lossy().into_owned()
                };
                file_obj.insert("path".to_string(), json!(path_str));
                file_obj.insert(
                    "extension".to_string(),
                    json!(entry.extension.clone().unwrap_or_default()),
                );
                file_obj.insert(
                    "code".to_string(),
                    json!(entry.code.clone().unwrap_or_default()),
                );
                if let Some(token_count) = entry.token_count {
                    file_obj.insert("token_count".to_string(), json!(token_count));
                }
                let metadata = EntryMetadata {
                    is_dir: false,
                    is_symlink: false,
                };
                file_obj.insert("metadata".to_string(), serde_json::to_value(metadata).ok()?);
                Some(serde_json::Value::Object(file_obj))
            })
            .collect();

        let mut data = json!({
            "absolute_code_path": crate::engine::traverse::label(&self.config.path),
            "source_tree": tree,
            "files": files_json,
        });

        // Add git data if the feature is enabled
        #[cfg(feature = "git")]
        {
            let data_obj = data.as_object_mut().unwrap();
            let git_diff = if git_diff_args.is_some() {
                get_git_diff(&self.config.path).unwrap_or_default()
            } else {
                String::new()
            };

            let git_diff_branch = if let Some((b1, b2)) = git_diff_branch_args {
                get_git_diff_between_branches(&self.config.path, b1, b2).unwrap_or_default()
            } else {
                String::new()
            };

            let git_log_branch = if let Some((b1, b2)) = git_log_branch_args {
                get_git_log(&self.config.path, b1, b2).unwrap_or_default()
            } else {
                String::new()
            };

            data_obj.insert("git_diff".to_string(), json!(git_diff));
            data_obj.insert("git_diff_branch".to_string(), json!(git_diff_branch));
            data_obj.insert("git_log_branch".to_string(), json!(git_log_branch));
        }

        Ok(data)
    }

    /// Renders a template with the given data.
    pub fn render_template(
        &self,
        handlebars: &Handlebars,
        template_name: &str,
        data: &serde_json::Value,
    ) -> Result<String> {
        let rendered = handlebars
            .render(template_name, data)
            .map_err(|e| anyhow::anyhow!("Failed to render template: {}", e))?;
        Ok(rendered.trim().to_string())
    }
}
