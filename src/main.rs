use anyhow::Result;
use clap::Parser;

// ──────────────────────────────────────────────────────────────
//  Entry point
// ──────────────────────────────────────────────────────────────
fn main() -> Result<()> {
   let args = code2prompt_tui::ui::cli::Cli::parse();
   code2prompt_tui::app_controller::run(args)
}
