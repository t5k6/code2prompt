use anyhow::Result;
use serde_json::json;

use crate::engine::{
    config::{Code2PromptConfig, OutputFormat, TokenFormat},
    model::ProcessedEntry,
    token::get_model_info,
};
use crate::ui::cli::Cli;
use crate::ui::template::write_to_file;

#[cfg(feature = "clipboard")]
use crate::ui::clipboard;

/// Handles all final output generation based on CLI arguments.
pub struct OutputHandler<'a> {
    rendered: &'a str,
    token_count: usize,
    processed_entries: &'a [ProcessedEntry],
    args: &'a Cli,
    config: &'a Code2PromptConfig,
}

impl<'a> OutputHandler<'a> {
    pub fn new(
        rendered: &'a str,
        token_count: usize,
        processed_entries: &'a [ProcessedEntry],
        args: &'a Cli,
        config: &'a Code2PromptConfig,
    ) -> Self {
        Self {
            rendered,
            token_count,
            processed_entries,
            args,
            config,
        }
    }

    pub fn handle(&self) -> Result<()> {
        #[cfg(feature = "token_map")]
        if self.args.token_map {
            self.handle_token_map()?;
        }

        #[cfg(not(feature = "token_map"))]
        if self.args.token_map {
            anyhow::bail!(
                "--token-map requires the 'token_map' feature, which was not included at compile time."
            );
        }

        if self.args.output_format == OutputFormat::Json {
            return self.handle_json_output(self.token_count);
        }

        if self.should_show_tokens() {
            self.display_token_count(self.token_count);
        }

        self.handle_final_output()
    }

    fn should_show_tokens(&self) -> bool {
        self.args.output_format != OutputFormat::Json && self.args.tokens == TokenFormat::Format
    }

    #[cfg(feature = "token_map")]
    fn handle_token_map(&self) -> Result<()> {
       // Move the necessary imports inside the conditionally compiled function.
       use crate::engine::token_map::generate_token_map_with_limit;
       use crate::ui::token_map_view;
       use terminal_size;
        let sum: usize = self
            .processed_entries
            .iter()
            .filter_map(|e| e.token_count)
            .sum();
        if sum > 0 {
            println!("\n[i] File Token Map (Sum of file tokens: {sum}):");
            let lines = self
                .args
                .token_map_lines
                .or_else(|| {
                    terminal_size::terminal_size().map(|(_, h)| (h.0 as usize).saturating_sub(10))
                })
                .unwrap_or(20)
                .max(5);
            let map = generate_token_map_with_limit(
                self.processed_entries,
                Some(lines),
                self.args.token_map_min_percent,
            );
            token_map_view::display_token_map(&map, sum);
        }
        Ok(())
    }

    fn handle_json_output(&self, total_tokens: usize) -> Result<()> {
        let paths: Vec<_> = self
            .processed_entries
            .iter()
            .map(|e| e.path.to_string_lossy().into_owned())
            .collect();

        let json_out = json!({
            "prompt": self.rendered,
            "directory_name": self.config.path.file_name().and_then(|s| s.to_str()).unwrap_or(""),
            "token_count": total_tokens,
            "model_info": get_model_info(self.config.tokenizer),
            "files": paths,
        });
        println!("{}", serde_json::to_string_pretty(&json_out)?);
        Ok(())
    }

    fn display_token_count(&self, total_tokens: usize) {
        #[cfg(feature = "token_map")]
        println!(
            "[i] Total Prompt Token count: {}, Model info: {}",
            total_tokens,
            get_model_info(self.config.tokenizer)
        );
        #[cfg(not(feature = "token_map"))]
        println!("[i] Token count unavailable: 'token_map' feature not enabled.");
    }

    fn handle_final_output(&self) -> Result<()> {
        let mut clipboard_ok = false;
        #[cfg(feature = "clipboard")]
        if !self.args.no_clipboard && clipboard::copy_to_clipboard(self.rendered).is_ok() {
            clipboard_ok = true;
            println!("[✓] Copied to clipboard.");
        }

        if let Some(path) = &self.args.output_file {
            write_to_file(path, self.rendered)?;
        } else if !clipboard_ok {
            println!(
                "\n--- PROMPT START ---\n{}\n--- PROMPT END ---",
                self.rendered
            );
        }
        Ok(())
    }
}

pub fn print_summary(path: &str, files: usize) {
    let line = "=".repeat(40);
    println!("\n{line}\n📂 Directory Processed: {path}\n📄 Files Processed: {files}\n{line}");
}
