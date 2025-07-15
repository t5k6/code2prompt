// src/args.rs

use crate::engine::config::{OutputFormat, TokenFormat};
use crate::engine::token::TokenizerChoice;
use clap::{Parser, ValueEnum};
use std::path::PathBuf;

// Define an enum for the sort argument for type safety
#[derive(ValueEnum, Debug, Clone, Default)] // <-- Add Default
pub enum FileSortMethod {
    #[default] // <-- Add #[default] attribute for clarity
    NameAsc,
    NameDesc,
    DateAsc,
    DateDesc,
}

// ~~~ CLI Arguments ~~~
#[derive(Parser, Debug, Clone)]
#[clap(
    name = env!("CARGO_PKG_NAME"),
    version = env!("CARGO_PKG_VERSION"),
    author = env!("CARGO_PKG_AUTHORS")
)]
#[command(arg_required_else_help = true)]
pub struct Cli {
    pub path: PathBuf,

    /// Patterns to include, comma-separated
    #[clap(short = 'i', long = "include", value_delimiter = ',')]
    pub include: Vec<String>,

    /// Patterns to exclude, comma-separated
    #[clap(short = 'e', long = "exclude", value_delimiter = ',')]
    pub exclude: Vec<String>,

    /// File extensions to include, comma-separated (e.g. "rs,toml")
    #[clap(long = "extensions", value_delimiter = ',')]
    pub extensions: Vec<String>,

    /// Include files in case of conflict between include and exclude patterns
    #[clap(long)]
    pub include_priority: bool,

    /// Optional output file path
    #[clap(short = 'O', long = "output-file")]
    pub output_file: Option<String>,

    /// Output format: markdown, json, or xml
    #[clap(short = 'F', long = "output-format", default_value_t = OutputFormat::Markdown)]
    pub output_format: OutputFormat,

    /// Optional Path to a custom Handlebars template
    #[clap(short = 'T', long)]
    pub template: Option<PathBuf>,

    /// List the full directory tree (opposite of current exclude_from_tree)
    #[clap(long)]
    pub full_directory_tree: bool,

    /// Tokenizer to use for token counting.
    ///
    /// Supported: o200k_base, cl100k
    #[clap(short = 't', long = "tokenizer", default_value_t = TokenizerChoice::Cl100k)]
    pub tokenizer: TokenizerChoice,

    /// Display the token count of the generated prompt.
    /// Accepts a format: "raw" (machine parsable) or "format" (human readable).
    #[clap(long, value_name = "FORMAT", default_value_t = TokenFormat::Format)]
    pub tokens: TokenFormat,

    #[clap(short, long)]
    pub diff: bool,

    /// Generate git diff between two branches
    #[clap(long, value_name = "BRANCHES", num_args = 2, value_delimiter = ',')]
    pub git_diff_branch: Option<Vec<String>>,

    /// Retrieve git log between two branches
    #[clap(long, value_name = "BRANCHES", num_args = 2, value_delimiter = ',')]
    pub git_log_branch: Option<Vec<String>>,

    /// Add line numbers to the source code
    #[clap(short, long)]
    pub line_numbers: bool,

    /// Use relative paths instead of absolute paths
    #[clap(long)]
    pub relative_paths: bool,

    /// Follow symlinks
    #[clap(short = 'L', long)]
    pub follow_symlinks: bool,

    /// Include hidden directories and files
    #[clap(long)]
    pub hidden: bool,

    /// Disable wrapping code inside markdown code blocks
    #[clap(long)]
    pub no_codeblock: bool,

    /// Disable copying to clipboard
    #[clap(long)]
    pub no_clipboard: bool,

    /// Skip .gitignore rules
    #[clap(long)]
    pub no_ignore: bool,

    /// Disable all interactive prompts (for use in scripts)
    #[clap(long)]
    pub no_interactive: bool,

    /// Sort order for files
    #[clap(long)]
    pub sort: Option<FileSortMethod>,

    /// Display a visual token map of files
    #[clap(long)]
    pub token_map: bool,

    /// Maximum number of lines to display in token map (default: 20)
    #[clap(long, value_name = "NUMBER")]
    pub token_map_lines: Option<usize>,

    /// Minimum percentage of tokens to display in token map (default: 0.1%)
    #[clap(long, value_name = "PERCENT")]
    pub token_map_min_percent: Option<f64>,

    #[arg(long, hide = true)]
    pub clipboard_daemon: bool,
}
