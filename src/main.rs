// src/main.rs

use anyhow::{Context, Result};
use clap::Parser;
use code2prompt::engine::config::{OutputFormat, TokenFormat};
use code2prompt::{
    engine::token::{get_model_info, get_tokenizer},
    ui::{
        cli::Cli,
        template::{handlebars_setup, prompt_for_undefined_variables, write_to_file},
    },
    Code2PromptConfigBuilder, Code2PromptSession,
};
use glob::Pattern;
use serde_json::json;
use std::path::PathBuf;

#[cfg(feature = "colors")]
use colored::{self, Colorize};
#[cfg(feature = "interactive")]
use indicatif::{ProgressBar, ProgressStyle};
#[cfg(feature = "interactive")]
use inquire::MultiSelect;
use log::warn;

// Constants
const DEFAULT_TEMPLATE_NAME: &str = "default";
const CUSTOM_TEMPLATE_NAME: &str = "custom";

// Main function remains the same as your original, just with updated use paths
// and session logic. The logic itself was already well-structured.
fn main() -> Result<()> {
    #[cfg(feature = "logging")]
    env_logger::init();
    let args = Cli::parse();

    if args.clipboard_daemon {
    #[cfg(feature = "clipboard")]
    {
        // The daemon logic is now self-contained in the copy_to_clipboard function
        code2prompt::ui::clipboard::copy_to_clipboard("", true)?;
    }
        return Ok(());
    }


    println!("Processing codebase at '{}'...\n", args.path.display());

    // --- Step 1: Determine include patterns from all sources ---
    let mut final_include_patterns = args.include.clone();

    let extension_patterns: Vec<String> = args
        .extensions
        .iter()
        .map(|ext| format!("**/*.{}", ext))
        .collect();
    final_include_patterns.extend(extension_patterns);

    let needs_interactive_selection = !args.no_interactive
        && final_include_patterns.is_empty();

    if needs_interactive_selection {
        #[cfg(feature = "interactive")]
        {
            // MODIFIED: Update spinner message
            let spinner = setup_spinner("Step 1: Scanning for file types & folders...");
            let scan_config = Code2PromptConfigBuilder::default()
                .path(args.path.clone())
                .exclude_patterns(
                    args.exclude
                        .iter()
                        .filter_map(|p| Pattern::new(p).ok())
                        .collect::<Vec<_>>(),
                )
                .no_ignore(args.no_ignore)
                .hidden(args.hidden)
                .follow_symlinks(args.follow_symlinks)
                .build()?;
            let mut scan_session = Code2PromptSession::new(scan_config)?;
            scan_session.scan_extensions()?;
            spinner.finish_with_message("Done!".green().to_string());

            let mut sorted_extensions: Vec<_> = scan_session.all_extensions.into_iter().collect();
            sorted_extensions.sort();

            let selected_extensions: Vec<String> = if sorted_extensions.is_empty() {
                if atty::is(atty::Stream::Stdout) {
                    println!("{}", "No files found to select.".yellow());
                } else {
                    println!("No files found to select.");
                }
                vec![]
            } else {
                MultiSelect::new(
                    "Step 2: Select file extensions (space to select, enter to confirm):",
                    sorted_extensions,
                )
                .with_help_message("Use space to toggle selections, then press enter.")
                .with_formatter(&|a| format!("{} extensions selected", a.len()))
                .prompt()?
            };

            let mut sorted_dirs: Vec<_> = scan_session.all_directories.into_iter().collect();
            sorted_dirs.sort_by(|a, b| a.0.cmp(&b.0));

            let selected_dirs: Vec<String> = if sorted_dirs.is_empty() {
                if atty::is(atty::Stream::Stdout) {
                    println!("{}", "No sub-directories found to select from.".yellow());
                }
                vec![]
            } else {
                let display_items: Vec<String> = sorted_dirs
                    .iter()
                    .map(|(d, count)| format!("{} ({})", d, count))
                    .collect();
                MultiSelect::new(
                    "Step 2b: Select folders (optional, space to select, enter to confirm):",
                    display_items,
                )
                .with_help_message("Use space to toggle, then press enter. Skip to include files from all folders.")
                .with_formatter(&|a| format!("{} folders selected", a.len()))
                .prompt()?
                .into_iter()
                // Safely strip the " (count)" part from the selection
                .map(|s| s.rsplit_once(' ').unwrap().0.trim_end().to_string())
                .collect()
            };

            if selected_extensions.is_empty() && selected_dirs.is_empty() {
                if atty::is(atty::Stream::Stdout) {
                    println!("{}", "No selections made. Exiting.".yellow());
                } else {
                    println!("No selections made. Exiting.");
                }
                return Ok(());
            }

            if !selected_dirs.is_empty() {
                // User picked folders. This is the primary filter.
                if selected_extensions.is_empty() {
                    // All files within selected folders
                    final_include_patterns.extend(selected_dirs.iter().map(|d| format!("{}/**", d)));
                } else {
                    // Specific extensions within selected folders (cross-product)
                    for dir in &selected_dirs {
                        for ext in &selected_extensions {
                            final_include_patterns.push(format!("{}/**/*.{}", dir, ext));
                        }
                    }
                }
            } else if !selected_extensions.is_empty() {
                // No folders selected, only extensions. Apply globally.
                 let selected_glob_patterns: Vec<String> = selected_extensions
                    .iter()
                    .map(|ext| format!("**/*.{}", ext))
                    .collect();
                final_include_patterns.extend(selected_glob_patterns);
            }
        }
    }

    if final_include_patterns.is_empty() {
        warn!("No include patterns were provided. No files will be processed. Use --include, --extensions, or interactive mode.");
    }

    // --- Step 2: Build the full configuration ---
    let include_patterns: Vec<_> = final_include_patterns
        .iter()
        .filter_map(|p| Pattern::new(p).ok())
        .collect();
    let exclude_patterns: Vec<_> = args
        .exclude
        .iter()
        .filter_map(|p| Pattern::new(p).ok())
        .collect();

    let config = Code2PromptConfigBuilder::default()
        .path(args.path.clone())
        .include_patterns(include_patterns)
        .exclude_patterns(exclude_patterns)
        .include_priority(args.include_priority)
        .line_numbers(args.line_numbers)
        .absolute_path(!args.relative_paths)
        .full_directory_tree(args.full_directory_tree)
        .no_codeblock(args.no_codeblock)
        .tokenizer(args.tokenizer)
        .token_map_enabled(args.token_map)
        .no_ignore(args.no_ignore)
        .hidden(args.hidden)
        .follow_symlinks(args.follow_symlinks)
        .sort(args.sort.clone())
        .build()?;

    // --- Step 3: Create and run the main session ---
    let mut session = Code2PromptSession::new(config)?;

    let files_processed_count = {
        #[cfg(feature = "interactive")]
        let spinner = setup_spinner("Step 3: Processing files...");
        #[cfg(not(feature = "interactive"))]
        println!("Step 3: Processing files...");

        session.process_codebase()?;

        #[cfg(feature = "interactive")]
        spinner.finish_with_message("Done!".green().to_string());
        #[cfg(not(feature = "interactive"))]
        println!("Done!");

        session.processed_entries.len() // This is the return value of the block
    };

    session.sort_files();

    // --- Step 4: Build template data ---
    #[cfg(feature = "interactive")]
    let spinner = setup_spinner("Step 4: Building final prompt...");
    #[cfg(not(feature = "interactive"))]
    println!("Step 4: Building final prompt...");

    let git_diff_args = if args.diff { Some("") } else { None };
    let git_diff_branch_args = args
        .git_diff_branch
        .as_ref()
        .and_then(|b| Some((b.first()?.as_str(), b.get(1)?.as_str())));
    let git_log_branch_args = args
        .git_log_branch
        .as_ref()
        .and_then(|b| Some((b.first()?.as_str(), b.get(1)?.as_str())));

    let mut data =
        session.build_template_data(git_diff_args, git_diff_branch_args, git_log_branch_args)?;

    #[cfg(feature = "interactive")]
    spinner.finish_with_message("Done!".green().to_string());
    #[cfg(not(feature = "interactive"))]
    println!("Done!");

    // --- Step 5: Render the template ---
    let (template_content, template_name) = get_template_path(&args.template)?;
    let handlebars = handlebars_setup(&template_content, template_name)?;

    if !args.no_interactive {
        #[cfg(feature = "interactive")]
        prompt_for_undefined_variables(&mut data, &template_content)?;
    }

    let rendered = session.render_template(&handlebars, template_name, &data)?;

    if rendered.trim().is_empty() && files_processed_count > 0 {
        warn!(
            "The generated prompt is empty despite processing {} files. Check your template file.",
            files_processed_count
        );
    } else if rendered.trim().is_empty() {
        // This case is now handled by the warning at the top
    }

    // --- Step 6: Handle output and visualization ---
    let display_total_tokens = args.output_format != OutputFormat::Json
        && args.tokens == TokenFormat::Format;

    let total_prompt_tokens = if args.output_format == OutputFormat::Json
        || display_total_tokens
        || args.token_map
    {
        let bpe = get_tokenizer(args.tokenizer)?;
        bpe.encode_with_special_tokens(&rendered).len()
    } else {
        0
    };

    if args.token_map {
        if let Some(files_json) = data.get("files").and_then(|f| f.as_array()) {
            let sum_file_tokens: usize = files_json
                .iter()
                .filter_map(|f| f.get("token_count").and_then(|tc| tc.as_u64()))
                .map(|tc| tc as usize)
                .sum();

            if sum_file_tokens > 0 {
                if atty::is(atty::Stream::Stdout) {
                    println!(
                        "\n{}{}{} File Token Map (Sum of file tokens: {}):",
                        "[".bold().white(),
                        "i".bold().blue(),
                        "]".bold().white(),
                        sum_file_tokens.to_string().bold().yellow()
                    );
                } else {
                    println!(
                        "\n[i] File Token Map (Sum of file tokens: {}):",
                        sum_file_tokens
                    );
                }

                // Calculate dynamic line count based on terminal height
                let max_lines = if let Some(lines) = args.token_map_lines {
                    lines
                } else {
                    // Default to terminal height minus some padding, or 20 if unavailable
                    terminal_size::terminal_size()
                        .map(|(_, terminal_size::Height(h))| (h as usize).saturating_sub(10))
                        .unwrap_or(20)
                        .max(5) // Ensure at least 5 lines
                };

                let token_map_entries = code2prompt::engine::token_map::generate_token_map_with_limit(
                    files_json,
                    Some(max_lines),
                    args.token_map_min_percent,
                );

                // Call the display function from the UI module
                code2prompt::ui::token_map_view::display_token_map(&token_map_entries, sum_file_tokens);

            } else {
                warn!("Token map was requested, but no files with token counts were found.");
            }
        }
    }

    if args.output_format == OutputFormat::Json {
        let paths_for_json: Vec<String> = data
            .get("files")
            .and_then(|f| f.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|file| {
                        file.get("path")
                            .and_then(|p| p.as_str())
                            .map(|s| s.to_string())
                    })
                    .collect()
            })
            .unwrap_or_default(); // or .unwrap_or(vec![])

        let json_output = json!({
            "prompt": rendered,
            "directory_name": data["absolute_code_path"].as_str().unwrap_or(""),
            "token_count": total_prompt_tokens,
            "model_info": get_model_info(args.tokenizer),
            "files": paths_for_json,
        });
        println!("{}", serde_json::to_string_pretty(&json_output)?);
        return Ok(());
    }

    println!();
    if display_total_tokens {
        if atty::is(atty::Stream::Stdout) {
            println!(
                "{}{}{} Total Prompt Token count: {}, Model info: {}",
                "[".bold().white(),
                "i".bold().blue(),
                "]".bold().white(),
                total_prompt_tokens.to_string().bold().yellow(),
                get_model_info(args.tokenizer)
            );
        } else {
            println!(
                "[i] Total Prompt Token count: {}, Model info: {}",
                total_prompt_tokens,
                get_model_info(args.tokenizer)
            );
        }
    }

    let mut clipboard_succeeded = false;
    if !args.no_clipboard {
        #[cfg(feature = "clipboard")]
        match code2prompt::ui::clipboard::copy_to_clipboard(&rendered, false) { // Pass false here
            Ok(_) => {
                if atty::is(atty::Stream::Stdout) {
                    println!(
                        "{}{}{} {}",
                        "[".bold().white(),
                        "✓".bold().green(),
                        "]".bold().white(),
                        "Copied to clipboard successfully.".green()
                    );
                } else {
                    println!("[✓] Copied to clipboard successfully.");
                }
                clipboard_succeeded = true;
            }
            Err(e) => {
                if atty::is(atty::Stream::Stdout) {
                    eprintln!(
                        "{}{}{} {}",
                        "[".bold().white(),
                        "!".bold().red(),
                        "]".bold().white(),
                        format!("Failed to copy to clipboard: {}", e).red()
                    );
                } else {
                    eprintln!("[!] Failed to copy to clipboard: {}", e);
                }
            }
        }
    }

    if let Some(output_path_str) = &args.output_file {
        write_to_file(output_path_str, &rendered)?;
    } else if !clipboard_succeeded {
        println!("\n--- PROMPT START ---\n{}\n--- PROMPT END ---", &rendered);
    }

    print_summary(&args.path.to_string_lossy(), files_processed_count);

    Ok(())
}

// These helper functions from the old main.rs can be kept at the bottom of the file
#[cfg(feature = "interactive")]
fn setup_spinner(message: &str) -> ProgressBar {
    let spinner = ProgressBar::new_spinner();
    spinner.enable_steady_tick(std::time::Duration::from_millis(120));
    spinner.set_style(
        ProgressStyle::default_spinner()
            .tick_strings(&["▹▹▹▹▹", "▸▹▹▹▹", "▹▸▹▹▹", "▹▹▸▹▹", "▹▹▹▸▹", "▹▹▹▹▸"])
            .template("{spinner:.blue} {msg}")
            .unwrap(),
    );
    spinner.set_message(message.to_string());
    spinner
}

fn get_template_path(template_path_opt: &Option<PathBuf>) -> Result<(String, &str)> {
    if let Some(template_path) = template_path_opt {
        let content = std::fs::read_to_string(template_path)
            .context("Failed to read custom template file")?;
        Ok((content, CUSTOM_TEMPLATE_NAME))
    } else {
        // Make sure 'default_template.hbs' is in the project root
        Ok((
            include_str!("../default_template.hbs").to_string(),
            DEFAULT_TEMPLATE_NAME,
        ))
    }
}

fn print_summary(path: &str, files_processed: usize) {
    // This condition correctly checks if the 'colors' feature is enabled AND if we are in a TTY.
    #[cfg(feature = "colors")]
    if atty::is(atty::Stream::Stdout) {
        // This is the "colors enabled" branch
        let equals_line = "=".repeat(40).dimmed().to_string();
        println!(
            "\n{}\n{} {}\n{} {}\n{}",
            equals_line,
            "📂 Directory Processed:".bold(),
            path,
            "📄 Files Processed:".bold(),
            files_processed.to_string().yellow(),
            equals_line
        );
        return; // Exit early to avoid running the non-colored version
    }

    // This is the "no colors" branch.
    // It will be executed if the 'colors' feature is disabled OR if stdout is not a TTY.
    let equals_line = "=".repeat(40);
    let summary = format!(
        "\n{}\n📂 Directory Processed: {}\n📄 Files Processed: {}\n{}",
        equals_line, path, files_processed, equals_line
    );
    println!("{}", summary);
}
