use anyhow::Result;
use handlebars::Handlebars;
#[cfg(any(feature = "cache", feature = "tui"))]
use rayon::prelude::*;
use serde_json::Value;

#[cfg(feature = "git")]
use crate::engine::git::{get_git_diff, get_git_diff_between_branches, get_git_log};
use crate::{
    Code2PromptConfigBuilder,
    common::{code, format, hash::HashMap},
    engine::{
        cache::ScanCache,
        config::Code2PromptConfig,
        model::{FileContext, ProcessedEntry, TemplateContext},
        traverse::{ProcessingMode, process_codebase},
    },
    ui::template::handlebars_setup,
};

/// Holds configuration and processed data for one “run”.
#[derive(Debug)]
pub struct Code2PromptSession {
    pub config: Code2PromptConfig,
    pub processed_entries: Vec<ProcessedEntry>,
    pub all_extensions: HashMap<String, usize>,
    pub all_directories: HashMap<String, usize>,
    #[cfg(any(feature = "cache", feature = "tui"))]
    scan_cache: Option<ScanCache>,
}

impl Code2PromptSession {
    // ──────────────────────────────────────────────────────────
    // Construction helpers
    // ──────────────────────────────────────────────────────────
    pub fn new(config: Code2PromptConfig) -> Result<Self> {
        #[cfg(any(feature = "cache", feature = "tui"))]
        let scan_cache = if config.cache {
            ScanCache::open(&config.path).ok()
        } else {
            None
        };
        Ok(Self {
            config,
            processed_entries: Vec::new(),
            all_extensions: HashMap::default(),
            all_directories: HashMap::default(),
            #[cfg(any(feature = "cache", feature = "tui"))]
            scan_cache,
        })
    }

    pub fn from_builder(builder: Code2PromptConfigBuilder) -> Result<Self> {
        Self::new(builder.build()?)
    }

    // ──────────────────────────────────────────────────────────
    // Scanning / processing
    // ──────────────────────────────────────────────────────────
    pub fn scan_extensions(&mut self) -> Result<()> {
        let (_, ext, dirs) = process_codebase(&self.config, ProcessingMode::ExtensionCollection)?;
        self.all_extensions = ext;
        self.all_directories = dirs;
        Ok(())
    }

    pub fn process_codebase(&mut self) -> Result<()> {
        let (entries, ext, dirs) = process_codebase(&self.config, ProcessingMode::FullProcess)?;
        self.processed_entries = entries;
        self.all_extensions = ext;
        self.all_directories = dirs;
        Ok(())
    }

    // ──────────────────────────────────────────────────────────
    // Sorting
    // ──────────────────────────────────────────────────────────
    pub fn sort_files(&mut self) {
        if let Some(m) = &self.config.sort {
            m.apply(&mut self.processed_entries)
        }
    }

    #[cfg(any(feature = "cache", feature = "tui"))]
    fn populate_code_jit(&mut self) -> Result<()> {
        let Some(cache) = &self.scan_cache else {
            return Ok(()); // Nothing to do if cache is disabled
        };

        // 1. Identify entries that need their code loaded.
        let entries_to_load = self
            .processed_entries
            .iter_mut()
            .filter(|e| e.code.is_none())
            .collect::<Vec<_>>();

        if entries_to_load.is_empty() {
            return Ok(());
        }

        // 2. Fetch all available content from the cache in a single batch query.
        let paths_to_query: Vec<&str> = entries_to_load
            .iter()
            .map(|e| e.relative_path.to_str().unwrap_or_default())
            .collect();
        let cached_contents = cache.get_cached_contents(&paths_to_query)?;

        // 3. Partition entries into those found in the cache and those requiring a disk read.
        let (cached_entries, disk_read_entries): (Vec<_>, Vec<_>) =
            entries_to_load.into_iter().partition(|e| {
                let path_str = e.relative_path.to_string_lossy();
                cached_contents.contains_key(path_str.as_ref())
            });

        // 4. Populate entries with cached content.
        for entry in cached_entries {
            let path_str = entry.relative_path.to_string_lossy();
            if let Some(content) = cached_contents.get(path_str.as_ref()) {
                entry.code = Some(code::wrap(
                    content,
                    entry.extension.as_deref().unwrap_or(""),
                    self.config.line_numbers,
                    self.config.no_codeblock,
                ));
            }
        }

        // 5. Read the remaining files from disk in parallel.
        let results: Vec<_> = disk_read_entries
            .into_par_iter()
            .filter_map(|entry| {
                std::fs::read_to_string(&entry.path).ok().map(|content| {
                    let wrapped_code = code::wrap(
                        &content,
                        entry.extension.as_deref().unwrap_or(""),
                        self.config.line_numbers,
                        self.config.no_codeblock,
                    );
                    (entry.path.clone(), wrapped_code)
                })
            })
            .collect();

        // Create a map for quick lookups and update the original entries.
        let disk_content_map: HashMap<_, _> = results.into_iter().collect();
        for entry in &mut self.processed_entries {
            if entry.code.is_none() {
                if let Some(wrapped_code) = disk_content_map.get(&entry.path) {
                    entry.code = Some(wrapped_code.clone());
                }
            }
        }

        Ok(())
    }

    // ──────────────────────────────────────────────────────────
    // Template-data builder
    // ──────────────────────────────────────────────────────────
    pub fn build_template_data(
        &mut self,
        git_diff: Option<&str>,
        git_diff_branch: Option<(&str, &str)>,
        git_log_branch: Option<(&str, &str)>,
    ) -> Result<TemplateContext> {
        // --- JIT Loading Step ---
        #[cfg(any(feature = "cache", feature = "tui"))]
        self.populate_code_jit()?;

        let files_context: Vec<FileContext> = self
            .processed_entries
            .iter()
            .filter(|e| e.is_file && e.code.is_some())
            .map(|e| {
                let path_val = if self.config.absolute_path {
                    e.path.to_string_lossy().into_owned()
                } else {
                    e.relative_path.to_string_lossy().into_owned()
                };
                FileContext {
                    path: path_val,
                    extension: e.extension.as_deref().unwrap_or("").to_string(),
                    code: e.code.as_deref().unwrap_or("").to_string(), // .unwrap() is safe due to filter
                    token_count: e.token_count,
                }
            })
            .collect();

        let mut context = TemplateContext {
            absolute_code_path: format::format_path_label(&self.config.path),
            files: files_context,
            source_tree: String::new(), // Populated later in main.rs
            git_diff: None,
            git_diff_branch: None,
            git_log_branch: None,
        };
        // Git extras (kept behind feature gate)
        #[cfg(feature = "git")]
        {
            context.git_diff =
                git_diff.map(|_| get_git_diff(&self.config.path).unwrap_or_default());
            context.git_diff_branch = git_diff_branch.map(|(a, b)| {
                get_git_diff_between_branches(&self.config.path, a, b).unwrap_or_default()
            });
            context.git_log_branch = git_log_branch
                .map(|(a, b)| get_git_log(&self.config.path, a, b).unwrap_or_default());
        }
        Ok(context)
    }

    pub fn render_prompt_and_count_tokens(
        &mut self,
        template_content: &str,
        template_name: &str,
        git_diff: Option<&str>,
        git_diff_branch: Option<(&str, &str)>,
        git_log_branch: Option<(&str, &str)>,
        user_vars_data: &Value,
    ) -> Result<(String, usize, Value)> {
        // 1. Sort files before rendering
        self.sort_files();

        // 2. Build the typed template context from current session state
        let context = self.build_template_data(git_diff, git_diff_branch, git_log_branch)?;

        // 3. Convert typed context to a generic Value for Handlebars
        let mut template_value = serde_json::to_value(context)?;

        // 4. Merge user-defined variables into the generic Value
        if let Some(obj) = template_value.as_object_mut() {
            if let Some(user_obj) = user_vars_data.as_object() {
                obj.extend(user_obj.clone());
            }
        }

        // 5. Set up Handlebars and render the template
        let hb = handlebars_setup(template_content, template_name)?;

        // Render with the current data
        let rendered = self.render_template(&hb, template_name, &template_value)?;

        // 6. Calculate tokens from the final rendered string
        let token_count = crate::engine::token::count_tokens(&rendered, self.config.tokenizer)?;

        Ok((rendered, token_count, template_value))
    }

    // ──────────────────────────────────────────────────────────
    // Template rendering
    // ──────────────────────────────────────────────────────────
    fn render_template(&self, hbs: &Handlebars, tpl_name: &str, data: &Value) -> Result<String> {
        hbs.render(tpl_name, data)
            .map(|s| s.trim().to_owned())
            .map_err(|e| anyhow::anyhow!("Failed to render template: {e}"))
    }
}
