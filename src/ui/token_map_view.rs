use crate::engine::model::TokenMapEntry;
use atty;
#[cfg(feature = "colors")]
use lscolors::{Indicator, LsColors, Style as LsStyle};
use std::cmp::Ordering;
use std::fmt::Write;
use std::path::Path;
use terminal_size;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

pub fn format_tokens_integer_arithmetic(tokens: usize) -> String {
    if tokens >= 1_000_000 {
        let millions = (tokens + 500_000) / 1_000_000;
        format!("{}M", millions)
    } else if tokens >= 1_000 {
        let thousands = (tokens + 500) / 1_000;
        format!("{}K", thousands)
    } else {
        format!("{}", tokens)
    }
}

#[cfg(feature = "colors")]
fn should_enable_colors() -> bool {
    if std::env::var("NO_COLOR").is_ok() {
        return false;
    }
    atty::is(atty::Stream::Stdout)
}

#[cfg(not(feature = "colors"))]
fn should_enable_colors() -> bool {
    false
}

/// Helper to manually construct ANSI escape codes from an lscolors::Style
#[cfg(feature = "colors")]
fn style_to_ansi(text: &str, style: &LsStyle) -> String {
    let mut s = String::new();
    let mut codes: Vec<String> = Vec::new();

    if style.font_style.bold {
        codes.push("1".to_string());
    }
    if style.font_style.dimmed {
        codes.push("2".to_string());
    }
    if style.font_style.italic {
        codes.push("3".to_string());
    }
    if style.font_style.underline {
        codes.push("4".to_string());
    }
    if style.font_style.strikethrough {
        codes.push("9".to_string());
    }

    if let Some(fg) = &style.foreground {
        match fg {
            lscolors::Color::Fixed(code) => codes.push(format!("38;5;{}", code)),
            lscolors::Color::RGB(r, g, b) => codes.push(format!("38;2;{};{};{}", r, g, b)),
            lscolors::Color::Black => codes.push("30".to_string()),
            lscolors::Color::Red => codes.push("31".to_string()),
            lscolors::Color::Green => codes.push("32".to_string()),
            lscolors::Color::Yellow => codes.push("33".to_string()),
            lscolors::Color::Blue => codes.push("34".to_string()),
            lscolors::Color::Magenta => codes.push("35".to_string()),
            lscolors::Color::Cyan => codes.push("36".to_string()),
            lscolors::Color::White => codes.push("37".to_string()),
            lscolors::Color::BrightBlack => codes.push("90".to_string()),
            lscolors::Color::BrightRed => codes.push("91".to_string()),
            lscolors::Color::BrightGreen => codes.push("92".to_string()),
            lscolors::Color::BrightYellow => codes.push("93".to_string()),
            lscolors::Color::BrightBlue => codes.push("94".to_string()),
            lscolors::Color::BrightMagenta => codes.push("95".to_string()),
            lscolors::Color::BrightCyan => codes.push("96".to_string()),
            lscolors::Color::BrightWhite => codes.push("97".to_string()),
        }
    }

    if codes.is_empty() {
        return text.to_string();
    }

    write!(s, "\x1b[{}m{}\x1b[0m", codes.join(";"), text).unwrap();
    s
}

fn generate_hierarchical_bar(
    bar_width: usize,
    parent_bar_str: &str,
    percentage: f64,
    depth: usize,
) -> String {
    if bar_width == 0 {
        return "".to_string();
    }
    let filled_chars = ((percentage / 100.0) * bar_width as f64).round() as usize;
    let mut result = String::new();
    let shade_char = match depth.max(1) {
        1 => ' ',
        2 => '░',
        3 => '▒',
        _ => '▓',
    };
    let parent_chars: Vec<char> = parent_bar_str.chars().collect();
    for i in 0..bar_width {
        if i < filled_chars {
            result.push('█');
        } else if i < parent_chars.len() {
            let parent_char = parent_chars[i];
            if parent_char == '█' {
                result.push(shade_char);
            } else {
                result.push(parent_char);
            }
        } else {
            result.push(' ');
        }
    }
    result
}

pub fn display_token_map(entries: &[TokenMapEntry], total_tokens: usize) {
    if entries.is_empty() {
        println!("No files to display in token map.");
        return;
    }
    #[cfg(feature = "colors")]
    let ls_colors = LsColors::from_env().unwrap_or_default();
    let colors_enabled = should_enable_colors();
    let terminal_width = terminal_size::terminal_size()
        .map(|(terminal_size::Width(w), _)| w as usize)
        .unwrap_or(80);
    let max_token_width = entries
        .iter()
        .map(|e| format_tokens_integer_arithmetic(e.tokens).len())
        .chain(std::iter::once(
            format_tokens_integer_arithmetic(total_tokens).len(),
        ))
        .max()
        .unwrap_or(3)
        .max(4);
    let max_depth_for_prefix = entries.iter().map(|e| e.depth).max().unwrap_or(0);
    let max_name_length = entries
        .iter()
        .map(|e| {
            let prefix_width = if e.depth == 0 { 3 } else { (e.depth * 2) + 3 };
            prefix_width + UnicodeWidthStr::width(e.name.as_str())
        })
        .max()
        .unwrap_or(20)
        .min(terminal_width / 2);
    let bar_width = terminal_width
        .saturating_sub(max_token_width + 3 + max_name_length + 2 + 2 + 5)
        .max(10);
    let mut parent_bars: Vec<String> = vec![String::new(); max_depth_for_prefix + 2];
    if bar_width > 0 {
        parent_bars[0] = "█".repeat(bar_width);
    }
    for (i, entry) in entries.iter().enumerate() {
        let mut prefix = String::new();
        for d_idx in 0..entry.depth {
            let mut has_sibling_below_at_d_idx_plus_1 = false;
            for next_entry in entries.iter().skip(i + 1) {
                if next_entry.depth < d_idx + 1 {
                    break;
                }
                if next_entry.depth == d_idx + 1 {
                    has_sibling_below_at_d_idx_plus_1 = true;
                    break;
                }
            }
            if has_sibling_below_at_d_idx_plus_1 {
                prefix.push_str("│ ");
            } else {
                prefix.push_str("  ");
            }
        }
        if entry.depth == 0 && i == 0 && entry.name != "(other files)" {
            prefix = "".to_string();
        }
        if entry.depth > 0 || (entry.depth == 0 && entry.name == "(other files)") {
            if entry.is_last {
                prefix.push_str("└─");
            } else {
                prefix.push_str("├─");
            }
        } else if i == 0 && entry.name != "(other files)" {
            prefix.push_str("┌─");
        }
        let has_children_to_display = entries
            .get(i + 1)
            .map(|next_entry| next_entry.depth > entry.depth)
            .unwrap_or(false);
        if entry.depth > 0 || entry.name == "(other files)" || i == 0
        {
            if has_children_to_display {
                prefix.push('┬');
            } else {
                prefix.push('─');
            }
        }
        prefix.push(' ');
        let tokens_str = format_tokens_integer_arithmetic(entry.tokens);
        let parent_bar_idx = if entry.depth == 0 { 0 } else { entry.depth - 1 };
        let parent_bar_to_use = parent_bars
            .get(parent_bar_idx)
            .cloned()
            .unwrap_or_else(|| " ".repeat(bar_width));
        let bar =
            generate_hierarchical_bar(bar_width, &parent_bar_to_use, entry.percentage, entry.depth);
        match entry.depth.cmp(&parent_bars.len()) {
            Ordering::Less => {
                parent_bars[entry.depth] = bar.clone();
            }
            Ordering::Equal | Ordering::Greater => {
                // If the depth is equal, we append. If it's greater, something is
                // wrong, but appending is the safest way to handle it to avoid a panic.
                parent_bars.push(bar.clone());
            }
        }
        let percentage_str = format!("{:>4.0}%", entry.percentage);
        let current_prefix_width = UnicodeWidthStr::width(prefix.as_str());
        let name_display_width = UnicodeWidthStr::width(entry.name.as_str());
        let available_for_name = max_name_length.saturating_sub(current_prefix_width);
        let (truncated_name, remaining_padding) = if name_display_width > available_for_name {
            let mut truncated_width = 0;
            let mut take_chars = 0;
            // Subtract 1 for the '…' character.
            let max_width = available_for_name.saturating_sub(1);
            for c in entry.name.chars() {
                let char_width = c.width().unwrap_or(0);
                if truncated_width + char_width > max_width {
                    break;
                }
                truncated_width += char_width;
                take_chars += 1;
            }
            let truncated: String = entry.name.chars().take(take_chars).collect();
            (format!("{}…", truncated), 0)
        } else {
            (entry.name.clone(), available_for_name - name_display_width)
        };
        let name_with_padding = format!("{}{}", truncated_name, " ".repeat(remaining_padding));

        let colored_name_with_padding = if colors_enabled && entry.name != "(other files)" {
            #[cfg(feature = "colors")]
            {
                let style = if entry.metadata.is_dir {
                    ls_colors.style_for_indicator(Indicator::Directory)
                } else {
                    // For files and symlinks, style_for_path is the most robust method
                    ls_colors.style_for_path(Path::new(&entry.path))
                }
                .cloned()
                .unwrap_or_default();

                style_to_ansi(&name_with_padding, &style)
            }
            #[cfg(not(feature = "colors"))]
            {
                // This block is unreachable if colors_enabled is false, but required for compilation
                name_with_padding.to_string()
            }
        } else {
            name_with_padding.to_string()
        };

        println!(
            "{:>max_token_width$}   {}{} │{}│ {}",
            tokens_str,
            prefix,
            colored_name_with_padding,
            bar,
            percentage_str,
            max_token_width = max_token_width
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_tokens_integer_arithmetic() {
        assert_eq!(format_tokens_integer_arithmetic(999), "999");
        assert_eq!(format_tokens_integer_arithmetic(1_000), "1K");
        assert_eq!(format_tokens_integer_arithmetic(1_499), "1K");
        assert_eq!(format_tokens_integer_arithmetic(1_500), "2K");
        assert_eq!(format_tokens_integer_arithmetic(1_501), "2K");
        assert_eq!(format_tokens_integer_arithmetic(1_999), "2K");
        assert_eq!(format_tokens_integer_arithmetic(1_000_000), "1M");
        assert_eq!(format_tokens_integer_arithmetic(2_499_999), "2M");
        assert_eq!(format_tokens_integer_arithmetic(2_500_000), "3M");
    }
}
