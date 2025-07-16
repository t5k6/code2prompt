#![cfg(feature = "clipboard")]

use anyhow::{Context, Result};
use arboard::Clipboard;

/// Copies text to the system clipboard.
/// This function relies on `arboard` to handle OS-specifics.
/// The `is_daemon` parameter is now ignored.
pub fn copy_to_clipboard(text: &str) -> Result<()> {
    let mut clipboard = Clipboard::new().context("Failed to initialize clipboard")?;
    clipboard
        .set_text(text.to_string())
        .context("Failed to copy to clipboard")
}
