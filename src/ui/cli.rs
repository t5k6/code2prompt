use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, ValueEnum};

use crate::engine::config::{OutputFormat, TokenFormat};
use crate::engine::model::ProcessedEntry;
use crate::engine::token::TokenizerChoice;

// Define an enum for the sort argument for type safety
#[derive(ValueEnum, Debug, Clone, Default, PartialEq, Eq)]
pub enum FileSortMethod {
    #[default]
    NameAsc,
    NameDesc,
    DateAsc,
    DateDesc,
}

impl FileSortMethod {
    pub fn apply(&self, v: &mut [ProcessedEntry]) {
        match self {
            Self::NameAsc => v.sort_by(|a, b| a.path.cmp(&b.path)),
            Self::NameDesc => v.sort_by(|a, b| b.path.cmp(&a.path)),
            Self::DateAsc => v.sort_by_key(|e| e.mtime),
            Self::DateDesc => v.sort_by_key(|e| std::cmp::Reverse(e.mtime)),
        }
    }
}

// ~~~ CLI Arguments ~~~
#[derive(Parser, Debug, Clone)]
#[clap(
    name = env!("CARGO_PKG_NAME"),
    version = env!("CARGO_PKG_VERSION"),
    author = env!("CARGO_PKG_AUTHORS")
)]
#[command(
    arg_required_else_help = true,
    after_help = r#"EXAMPLES:
    code2prompt .
        Scans the current directory interactively.
    code2prompt . --extensions rs,toml
        Includes only files with .rs and .toml extensions.
    code2prompt /path/to/project -e '**/tests/*_snapshots/*'
        Scans a different path and excludes snapshot files from tests."
    code2prompt . --extensions rs,toml --no-interactive
        Include only Rust and TOML files non-interactively
    code2prompt . -e "tests/**" -F json
        Exclude the 'tests' directory and generate a JSON output
    code2prompt . --diff -O prompt.txt
        Get a diff of the current branch and send it to an output file
  "#
)]
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

    /// Inline template variable, e.g., -V issue=123 -V author="Ada L." (repeatable)
    #[clap(short = 'V', long = "var", value_parser = parse_key_val, number_of_values = 1)]
    pub vars: Vec<(String, String)>,

    /// Path to a TOML/JSON/YAML file containing template variables.
    #[clap(long = "vars-file")]
    pub vars_file: Option<PathBuf>,

    /// List discovered templates and exit.
    #[clap(long = "list-templates")]
    pub list_templates: bool,

    /// Skip reading or writing cached variable answers.
    #[clap(long = "no-var-cache")]
    pub no_var_cache: bool,

    /// List the full directory tree (opposite of current exclude_from_tree)
    #[clap(long)]
    pub full_directory_tree: bool,

    /// Tokenizer to use for token counting.
    ///
    /// Supported: o200k_base, cl100k
    #[clap(short = 't', long = "tokenizer")]
    pub tokenizer: Option<TokenizerChoice>,

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

    /// Disable the default exclude patterns (.git, target/, etc.)
    #[clap(long)]
    pub no_default_excludes: bool,

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

    /// [DEBUG] Print the experimental directory tree and exit
    #[clap(long, hide = true)]
    pub experimental_tree: bool,

    /// Minimum percentage of tokens to display in token map (default: 0.1%)
    #[clap(long, value_name = "PERCENT")]
    pub token_map_min_percent: Option<f64>,

    #[clap(long)]
    pub cache: bool,
}

/// A clap value-parser for `-V key=value` arguments.
fn parse_key_val(s: &str) -> Result<(String, String), String> {
    s.split_once('=')
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .ok_or_else(|| "Variable must be in KEY=value format".to_string())
}
