use anyhow::{Context, Result};
use glob::Pattern;

use crate::engine::{config::Code2PromptConfigBuilder, config_file, token::TokenizerChoice};
use crate::ui::cli::Cli;

const DEFAULT_EXCLUDES: &[&str] = &[
    ".git/",
    ".svn/",
    ".hg/",
    "*.exe",
    "*.dll",
    "target/",
    "node_modules/",
];

pub fn build_config_builder(
    args: &Cli,
    cfg_file: &config_file::ConfigFile,
    extra: impl FnOnce(&mut Code2PromptConfigBuilder),
) -> Code2PromptConfigBuilder {
    let mut b = Code2PromptConfigBuilder::default();
    b.path(args.path.clone())
        .line_numbers(args.line_numbers || cfg_file.line_numbers.unwrap_or(false))
        .absolute_path(!args.relative_paths)
        .full_directory_tree(args.full_directory_tree)
        .no_codeblock(args.no_codeblock || cfg_file.no_codeblock.unwrap_or(false))
        .tokenizer(
            args.tokenizer
                .or(cfg_file.tokenizer)
                .unwrap_or(TokenizerChoice::Cl100k),
        )
        .hidden(args.hidden)
        .no_ignore(args.no_ignore)
        .follow_symlinks(args.follow_symlinks)
        .include_priority(args.include_priority)
        .sort(args.sort.clone())
        .cache(args.cache);

    extra(&mut b);
    b
}

pub fn build_include_patterns(args: &Cli) -> Vec<String> {
    let mut inc = args.include.clone();
    inc.extend(args.extensions.iter().map(|e| format!("**/*.{e}")));
    inc
}

pub fn build_exclude_patterns(
    args: &Cli,
    cfg_file: &config_file::ConfigFile,
    with_defaults: bool,
) -> Vec<String> {
    let mut ex = cfg_file.exclude.clone().unwrap_or_default();
    ex.extend(args.exclude.clone());
    if with_defaults && !(args.no_default_excludes || cfg_file.no_default_excludes.unwrap_or(false))
    {
        ex.extend(DEFAULT_EXCLUDES.iter().map(|s| s.to_string()));
    }
    ex
}

pub fn patterns_from_strings(v: &[String]) -> Result<Vec<Pattern>> {
    v.iter()
        .map(|p| Pattern::new(p).with_context(|| format!("Invalid glob pattern: '{p}'")))
        .collect()
}

pub fn needs_interactive_tui(args: &Cli) -> bool {
    #[cfg(feature = "tui")]
    {
        !args.no_interactive && args.include.is_empty() && args.extensions.is_empty()
    }
    #[cfg(not(feature = "tui"))]
    {
        false
    }
}
