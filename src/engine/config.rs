// src/config.rs

use crate::engine::token::TokenizerChoice;
use crate::ui::cli::FileSortMethod;
use clap::ValueEnum;
use derive_builder::Builder;
use glob::Pattern;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq, Default)]
pub enum OutputFormat {
    #[default]
    Markdown,
    Json,
    Xml,
}

impl std::fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OutputFormat::Markdown => write!(f, "markdown"),
            OutputFormat::Json => write!(f, "json"),
            OutputFormat::Xml => write!(f, "xml"),
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq, Default)]
pub enum TokenFormat {
    #[default]
    Format,
    Raw,
}

impl std::fmt::Display for TokenFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenFormat::Format => write!(f, "format"),
            TokenFormat::Raw => write!(f, "raw"),
        }
    }
}

#[derive(Debug, Clone, Builder)]
#[builder(setter(into), build_fn(name = "build_internal"))]
pub struct Code2PromptConfig {
    #[builder(default = "PathBuf::from(\".\")")]
    pub path: PathBuf,

    #[builder(default)]
    pub include_patterns: Vec<Pattern>,

    #[builder(default)]
    pub exclude_patterns: Vec<Pattern>,

    #[builder(default)]
    pub include_priority: bool,

    #[builder(default)]
    pub line_numbers: bool,
    #[builder(default)]
    pub absolute_path: bool,
    #[builder(default)]
    pub full_directory_tree: bool,
    #[builder(default)]
    pub no_codeblock: bool,
    #[builder(default = "TokenizerChoice::Cl100k")]
    pub tokenizer: TokenizerChoice,
    #[builder(default)]
    pub token_map_enabled: bool,
    #[builder(default)]
    pub no_ignore: bool,
    #[builder(default)]
    pub hidden: bool,
    #[builder(default)]
    pub follow_symlinks: bool,
    #[builder(default)]
    pub sort: Option<FileSortMethod>,
}

impl Code2PromptConfigBuilder {
    pub fn build(&self) -> Result<Code2PromptConfig, Code2PromptConfigBuilderError> {
        self.build_internal()
    }
}
