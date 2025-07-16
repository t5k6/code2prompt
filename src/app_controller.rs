use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rustc_hash::FxHashMap;
use serde_json::Value;

use crate::{
    Code2PromptSession,
    common::{cache::CacheManager, hash::HashMap},
    engine::{
        cache::{TemplateVariables, load_vars_from_file},
        config::Code2PromptConfigBuilder,
        config_file,
        token::count_tokens,
    },
    ui::{
        cache,
        cli::Cli,
        config::{
            build_config_builder, build_exclude_patterns, build_include_patterns,
            needs_interactive_tui, patterns_from_strings,
        },
        output, template,
        tree_arena::DirNode,
        tree_view::build_tree_view,
        tui_select::{TuiAction, TuiSettings},
    },
};

// Gated imports for TUI features
#[cfg(feature = "tui")]
use {
    crate::ui::{tree_arena::build_dir_arena, tui_select},
    std::collections::HashSet,
};

// Gated imports for colors feature
#[cfg(feature = "colors")]
use colored::{ColoredString, Colorize};

/// The primary orchestration function for the application.
pub fn run(args: Cli) -> Result<()> {
    let (tpl_content, tpl_hash) = template::resolve_template(&args.path, &args.template)?;

    if args.list_templates {
        println!("Template Search Order:");
        println!("1. --template <path>");
        println!(
            "2. Project-local: {}",
            args.path.join(".code2prompt/template.hbs").display()
        );
        println!(
            "3. User-global:  {}",
            dirs::config_dir()
                .unwrap_or_default()
                .join("code2prompt/template.hbs")
                .display()
        );
        println!("4. Built-in Default");
        println!(
            "\nCurrently using: {}",
            if tpl_hash == "builtin" {
                "Built-in Default".to_string()
            } else {
                format!("Custom template (hash: {})", &tpl_hash[..12])
            }
        );
        return Ok(());
    }

    let cache_manager = CacheManager::new(&args.path)?;
    let cfg_file: config_file::ConfigFile =
        confy::load("code2prompt", None).context("Failed to load config file")?;

    // --- START: Variable Merging ---
    let mut vars_map = HashMap::<String, String>::default();

    if !args.no_var_cache {
        if let Some(cached) = cache_manager.load::<TemplateVariables>()? {
            vars_map.extend(cached.0);
        }
    }

    if let Some(defaults) = &cfg_file.template.defaults {
        for (k, v) in defaults {
            vars_map.entry(k.clone()).or_insert_with(|| v.clone());
        }
    }

    for (key, value) in std::env::vars().filter(|(k, _)| k.starts_with("C2P_")) {
        let key = key.trim_start_matches("C2P_").to_lowercase();
        vars_map.entry(key).or_insert(value);
    }

    if let Some(path) = &args.vars_file {
        for (k, v) in load_vars_from_file(path)? {
            vars_map.insert(k, v);
        }
    }

    for (key, value) in &args.vars {
        vars_map.insert(key.clone(), value.clone());
    }

    // --- END: Variable Merging ---

    let placeholders = template::extract_placeholders(&tpl_content)?;
    let missing_vars: Vec<String> = placeholders
        .into_iter()
        .filter(|p| !vars_map.contains_key(p))
        .collect();

    #[cfg(feature = "interactive")]
    if !missing_vars.is_empty() && !args.no_interactive {
        println!("{}", colour("[i] Your template requires some variables."));
        let new_vars = template::prompt_for_variables(&missing_vars, &vars_map)?;
        vars_map.extend(new_vars);
        if !args.no_var_cache {
            cache_manager.save(&TemplateVariables(vars_map.clone()))?;
        }
    }

    let user_vars_data: Value = serde_json::to_value(vars_map)?;

    let mut session = if needs_interactive_tui(&args) {
        #[cfg(feature = "tui")]
        {
            run_interactive_flow(&args, &cache_manager, &cfg_file)?
        }
        #[cfg(not(feature = "tui"))]
        {
            anyhow::bail!(
                "Interactive mode requires the 'tui' feature. Please provide include/extension filters."
            )
        }
    } else {
        run_batch_flow(&args, &cfg_file)?
    };

    let mut context = session.build_template_data(
        args.diff.then_some(""),
        parse_branch_pair(&args.git_diff_branch),
        parse_branch_pair(&args.git_log_branch),
    )?;

    // 2. Generate and inject the source tree string into the context
    context.source_tree = build_tree_view(
        &session.config.path,
        &session.processed_entries,
        session.config.full_directory_tree,
    );

    let mut template_value = serde_json::to_value(context)?;
    if let Some(obj) = template_value.as_object_mut() {
        if let Some(user_obj) = user_vars_data.as_object() {
            obj.extend(user_obj.clone());
        }
    }

    let tpl_render_name = if tpl_hash == "builtin" {
        "default"
    } else {
        "custom"
    };
    let hb = template::handlebars_setup(&tpl_content, tpl_render_name)?;
    let rendered = hb
        .render(tpl_render_name, &template_value)
        .map(|s| s.trim().to_string())
        .map_err(|e| anyhow::anyhow!("Failed to render template: {e}"))?;

    let token_count = count_tokens(&rendered, session.config.tokenizer)?;

    let handler = output::OutputHandler::new(
        &rendered,
        token_count,
        &session.processed_entries,
        &args,
        &session.config,
    );
    handler.handle()?;

    output::print_summary(
        &session.config.path.to_string_lossy(),
        session.processed_entries.len(),
    );

    Ok(())
}

// ──────────────────────────────────────────────────────────────
//  Batch flow (non-interactive)
// ──────────────────────────────────────────────────────────────
fn run_batch_flow(args: &Cli, cfg_file: &config_file::ConfigFile) -> Result<Code2PromptSession> {
    let includes = build_include_patterns(args);
    let excludes = build_exclude_patterns(args, cfg_file, true);
    create_and_process_session(
        args,
        cfg_file,
        &includes,
        &excludes,
        args.token_map, // Pass through whether token map is enabled
        None,           // No extra builder function for batch mode
    )
}

// ──────────────────────────────────────────────────────────────
//  Interactive flow (TUI selector)
// ──────────────────────────────────────────────────────────────
#[cfg(feature = "tui")]
fn run_interactive_flow(
    args: &Cli,
    cache_manager: &CacheManager,
    cfg_file: &config_file::ConfigFile,
) -> Result<Code2PromptSession> {
    // This logic is now handled inside `select_filters_tui` and its caller
    // by correctly constructing `initial_config`. So we can simplify this.
    let mut current_settings: Option<TuiSettings> = None;

    loop {
        let (mut session, sorted_ext, dir_arena) =
            prepare_interactive_data(args, cfg_file, current_settings.as_ref())?;

        // `session.config` now holds the right initial values.
        let last_sel_opt = cache_manager.load::<cache::LastSelection>()?;
        let action = tui_select::select_filters_tui(
            &args.path,
            sorted_ext,
            dir_arena,
            last_sel_opt,
            &session.config, // We pass the fully-formed config here
        )?;
        println!();

        // ---- 3. Process the action ----
        match action {
            TuiAction::Confirm { exts, paths } => {
                let new_selection = cache::LastSelection {
                    extensions: exts.clone(),
                    directories: paths
                        .iter()
                        .map(|p| p.to_string_lossy().into_owned())
                        .collect(),
                };
                cache_manager.save(&new_selection)?;

                if exts.is_empty() && paths.is_empty() {
                    println!("{}", colour("No selections made. Exiting."));
                    std::process::exit(0);
                }

                filter_session_entries(&mut session, &exts, &paths);
                return Ok(session);
            }
            TuiAction::RescanWithConfig {
                settings,
                show_msg: _,
            } => {
                let new_settings = settings.clone();
                let mut cfg_edit = cfg_file.clone();
                cfg_edit.gui.settings = new_settings.clone();
                let _ = confy::store("code2prompt", None, cfg_edit);
                current_settings = Some(new_settings);
                continue;
            }
            TuiAction::Cancel => {
                println!("{}", colour("No selections made. Exiting."));
                std::process::exit(0);
            }
        }
    }
}

#[cfg(feature = "tui")]
fn prepare_interactive_data(
    args: &Cli,
    cfg_file: &config_file::ConfigFile,
    overrides: Option<&TuiSettings>,
) -> Result<(Code2PromptSession, Vec<(String, usize)>, Vec<DirNode>)> {
    println!("Scanning files for interactive selection…");

    // Create a closure to apply settings overrides to the config builder.
    let builder_ext = |b: &mut Code2PromptConfigBuilder| {
        if let Some(o) = overrides {
            b.line_numbers(o.line_numbers)
                .hidden(o.hidden)
                .follow_symlinks(o.follow_symlinks)
                .no_codeblock(o.no_codeblock)
                .tokenizer(o.tokenizer);
        }
    };

    let _include_patterns: &[String] = &[];
    let excludes = build_exclude_patterns(args, cfg_file, true);

    let session = create_and_process_session(
        args,
        cfg_file,
        &[],       // include_patterns
        &excludes, // Use the cached result
        true,
        Some(&builder_ext),
    )?;

    // The rest of the logic remains the same.
    let by_ext: HashMap<String, usize> = session
        .processed_entries
        .iter()
        .filter_map(|e| Some((e.extension.clone()?, e.token_count?)))
        .fold(HashMap::default(), |mut m, (ext, tok)| {
            *m.entry(ext).or_default() += tok;
            m
        });
    let mut sorted_ext: Vec<_> = by_ext.into_iter().collect();
    sorted_ext.sort_by(|a, b| b.1.cmp(&a.1));

    let mut ext_to_slot: FxHashMap<String, u16> = FxHashMap::default();
    for (i, (ext, _)) in sorted_ext.iter().enumerate() {
        ext_to_slot.insert(ext.clone(), (i + 1) as u16);
    }

    let dir_arena = build_dir_arena(&session.processed_entries, &ext_to_slot);

    Ok((session, sorted_ext, dir_arena))
}

// Extracted filtering logic for clarity and testing
#[cfg(feature = "tui")]
pub fn filter_session_entries(
    session: &mut Code2PromptSession,
    sel_exts: &[String],
    sel_paths: &[PathBuf],
) {
    // Correctly create a HashSet<String> for efficient and correct lookups.
    let ext_set: HashSet<String> = sel_exts.iter().cloned().collect();

    session.processed_entries.retain(|e| {
        let matches_extension = if ext_set.is_empty() {
            true
        } else {
            e.extension
                .as_deref()
                .map_or(false, |ext| ext_set.contains(ext))
        };

        let matches_path = if sel_paths.is_empty() {
            true
        } else {
            let rel_path = &e.relative_path;
            sel_paths
                .iter()
                .any(|p| paths_match_case_insensitive(rel_path, p))
        };

        // The file is kept only if it meets BOTH specified criteria.
        matches_extension && matches_path
    });
}

// ──────────────────────────────────────────────────────────────
//  Helpers (config merging, patterns, template, summary)
// ──────────────────────────────────────────────────────────────

fn create_and_process_session(
    args: &Cli,
    cfg_file: &config_file::ConfigFile,
    include_patterns: &[String],
    exclude_patterns: &[String],
    token_map_enabled: bool,
    // Use a simpler, immutable function reference.
    extra_builder_fn: Option<&dyn Fn(&mut Code2PromptConfigBuilder)>,
) -> Result<Code2PromptSession> {
    let include = patterns_from_strings(include_patterns)?;
    let exclude = patterns_from_strings(exclude_patterns).unwrap_or_else(|e| {
        #[cfg(feature = "logging")]
        log::warn!("Ignoring invalid exclude pattern: {}", e);
        Vec::new()
    });

    // Pass the extra closure directly into build_config_builder.
    let mut builder = build_config_builder(args, cfg_file, |b| {
        b.include_patterns(include.clone());
        b.exclude_patterns(exclude.clone());
        if let Some(extra_fn) = extra_builder_fn {
            extra_fn(b);
        }
    });

    let config = builder
        .token_map_enabled(token_map_enabled)
        .build()
        .context("Failed to build configuration for session")?;

    let mut session = Code2PromptSession::new(config)?;
    session.process_codebase()?;
    Ok(session)
}

#[cfg(feature = "colors")]
fn colour<S: AsRef<str>>(s: S) -> ColoredString {
    s.as_ref().yellow()
}
#[cfg(not(feature = "colors"))]
fn colour<S: AsRef<str>>(s: S) -> String {
    s.as_ref().into()
}

/// Parses a clap argument of Option<Vec<String>> into a tuple of string slices.
fn parse_branch_pair(branches: &Option<Vec<String>>) -> Option<(&str, &str)> {
    branches.as_ref().and_then(|v| {
        if let [a, b] = v.as_slice() {
            Some((a.as_str(), b.as_str()))
        } else {
            None
        }
    })
}

#[cfg(feature = "tui")]
fn paths_match_case_insensitive(full_path: &Path, prefix: &Path) -> bool {
    let mut full_components = full_path.components();
    let mut prefix_components = prefix.components();

    loop {
        match (prefix_components.next(), full_components.next()) {
            (Some(p_comp), Some(f_comp)) => {
                // Compare components case-insensitively.
                if !p_comp.as_os_str().eq_ignore_ascii_case(f_comp.as_os_str()) {
                    return false; // Mismatch found.
                }
            }
            (Some(_), None) => return false, // `full_path` is shorter than `prefix`.
            (None, _) => return true,        // `prefix` is a valid prefix of `full_path`.
        }
    }
}
