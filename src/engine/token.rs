//! This module encapsulates the logic for counting the tokens in the rendered text.

use anyhow::Result;
use clap::ValueEnum;
use dashmap::DashMap;
use once_cell::sync::OnceCell;
use std::sync::Arc;
use tiktoken_rs::{get_bpe_from_tokenizer, tokenizer::Tokenizer, CoreBPE};

type SharedBPE = Arc<CoreBPE>;
static TOKENIZER_CACHE: OnceCell<DashMap<String, SharedBPE>> = OnceCell::new();

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum TokenizerChoice {
    /// For GPT-4o, GPT-4 Turbo, and o1 models.
    O200kBase,
    /// For ChatGPT models, text-embedding-ada-002. (Default)
    Cl100k,
    /// For Code models, text-davinci-002, text-davinci-003.
    P50kBase,
    /// For Edit models like text-davinci-edit-001.
    P50kEdit,
    /// For GPT-3 models like davinci.
    #[value(name = "r50k_base", alias = "gpt2")]
    R50kBase,
}

impl std::fmt::Display for TokenizerChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenizerChoice::O200kBase => write!(f, "o200k_base"),
            TokenizerChoice::Cl100k => write!(f, "cl100k"),
            TokenizerChoice::P50kBase => write!(f, "p50k_base"),
            TokenizerChoice::P50kEdit => write!(f, "p50k_edit"),
            TokenizerChoice::R50kBase => write!(f, "r50k_base"),
        }
    }
}

fn get_cache() -> &'static DashMap<String, SharedBPE> {
    TOKENIZER_CACHE.get_or_init(DashMap::new)
}
/// Returns the appropriate tokenizer based on the provided encoding.
///
/// # Arguments
///
/// * `encoding` - An optional string specifying the encoding to use for tokenization.
///                Supported encodings: "o200k_base", "cl100k" (default).
///
/// # Returns
///
/// * `CoreBPE` - The tokenizer corresponding to the specified encoding.
pub fn get_tokenizer(tokenizer_name: TokenizerChoice) -> Result<SharedBPE> {
    // <-- Use the enum
    let cache = get_cache();
    let name_str = tokenizer_name.to_string(); // Use the display impl to get the string for the cache key
    if let Some(bpe) = cache.get(&name_str) {
        return Ok(bpe.clone());
    }

    let tokenizer_enum = match tokenizer_name {
        TokenizerChoice::O200kBase => Tokenizer::O200kBase,
        TokenizerChoice::Cl100k => Tokenizer::Cl100kBase,
        TokenizerChoice::P50kBase => Tokenizer::P50kBase,
        TokenizerChoice::P50kEdit => Tokenizer::P50kEdit,
        TokenizerChoice::R50kBase => Tokenizer::R50kBase,
    };

    let bpe_result = get_bpe_from_tokenizer(tokenizer_enum).map_err(|e| anyhow::anyhow!(e))?;
    let bpe_arc = Arc::new(bpe_result);

    cache.insert(name_str, bpe_arc.clone());

    Ok(bpe_arc)
}

/// Returns the model information based on the provided encoding.
///
/// # Arguments
///
/// * `encoding` - An optional string specifying the encoding to use for retrieving model information.
///                Supported encodings: "o200k_base", "cl100k" (default).
///
/// # Returns
///
/// * `&'static str` - A string describing the models associated with the specified encoding.
pub fn get_model_info(tokenizer_name: TokenizerChoice) -> &'static str {
    // <-- Use the enum
    match tokenizer_name {
        TokenizerChoice::O200kBase => "GPT-4o models, o1 models",
        TokenizerChoice::Cl100k => "ChatGPT models, text-embedding-ada-002",
        TokenizerChoice::P50kBase => "Code models, text-davinci-002, text-davinci-003",
        TokenizerChoice::P50kEdit => {
            "Edit models like text-davinci-edit-001, code-davinci-edit-001"
        }
        TokenizerChoice::R50kBase => "GPT-3 models like davinci",
    }
}

/// Counts the tokens in the rendered text using the specified encoding.
///
/// # Arguments
///
/// * `text` - The text to count tokens for.
/// * `encoding` - An optional string specifying the encoding to use for token counting.
///
/// # Returns
///
/// * `usize` - The number of tokens in the text.
pub fn count_tokens(text: &str, tokenizer_name: TokenizerChoice) -> Result<usize> {
    // <-- Use the enum
    let bpe = get_tokenizer(tokenizer_name)?;
    Ok(bpe.encode_with_special_tokens(text).len())
}
