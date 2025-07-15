// src/lib.rs

//! Internal library for code2prompt – not published on crates.io

pub mod engine;
pub mod ui;

// Re-export a narrow, testable API surface
pub use engine::{
    config::{Code2PromptConfig, Code2PromptConfigBuilder},
    model::{ProcessedEntry, TokenMapEntry},
    session::Code2PromptSession,
    token::{count_tokens, TokenizerChoice},
};
