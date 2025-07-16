//! A centralized module for user-facing formatting utilities.

use std::path::Path;

use thousands::Separable;

/// Defines the style for formatting token counts.
pub enum TokenFormatStyle {
    /// A compact format suitable for narrow TUI columns (e.g., "3.2k", "11k").
    Compact,
    /// A format suitable for the visual token map (e.g., "123K", "2M").
    Map,
}

/// Formats a token count according to a specific style.
pub fn format_tokens(n: usize, style: TokenFormatStyle) -> String {
    match style {
        TokenFormatStyle::Compact => match n {
            0..=999 => n.separate_with_commas(),
            1_000..=9_999 => format!("{:.1}k", n as f64 / 1_000.0),
            _ => format!("{:.0}k", n as f64 / 1_000.0),
        },
        TokenFormatStyle::Map => {
            if n >= 1_000_000 {
                let millions = (n + 500_000) / 1_000_000;
                format!("{millions}M")
            } else if n >= 1_000 {
                let thousands = (n + 500) / 1_000;
                format!("{thousands}K")
            } else {
                format!("{n}")
            }
        }
    }
}

/// Creates a user-friendly label for a path.
/// Used in UI headers and tree roots. Takes the file_name, or if that's
/// missing, the last component of the current directory.
pub fn format_path_label<P: AsRef<Path>>(p: P) -> String {
    let path = p.as_ref();
    if path.file_name().is_none() {
        std::env::current_dir()
            .ok()
            .and_then(|d| d.file_name().and_then(|n| n.to_str().map(str::to_owned)))
            .unwrap_or_else(|| ".".into())
    } else {
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_owned()
    }
}
