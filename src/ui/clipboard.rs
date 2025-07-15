// src/ui/clipboard.rs
#![cfg(feature = "clipboard")]

use anyhow::{Context, Result};
use arboard::Clipboard;
use std::io::{self, Read};

/// Copies text to the system clipboard, using a daemon on Linux.
pub fn copy_to_clipboard(text: &str, is_daemon: bool) -> Result<()> {
    if is_daemon {
        #[cfg(target_os = "linux")]
        return serve_clipboard_daemon();
    } else {
        #[cfg(target_os = "linux")]
        return spawn_clipboard_daemon(text);
    }

    #[cfg(not(target_os = "linux"))]
    {
        let mut clipboard = Clipboard::new().context("Failed to initialize clipboard")?;
        clipboard
            .set_text(text.to_string())
            .context("Failed to copy to clipboard")
    }
}

#[cfg(target_os = "linux")]
fn spawn_clipboard_daemon(text: &str) -> Result<()> {
    use std::process::{Command, Stdio};

    let mut child = Command::new(std::env::current_exe()?)
        .arg("--clipboard-daemon")
        .stdin(Stdio::piped())
        .spawn()
        .context("Failed to spawn clipboard daemon")?;

    let mut stdin = child.stdin.take().unwrap();

    // Create an owned String from the &str reference.
    let text_owned = text.to_string();

    // Now, move the owned String into the thread.
    std::thread::spawn(move || {
        use std::io::Write;
        // The thread now owns `text_owned` and can safely use it.
        let _ = stdin.write_all(text_owned.as_bytes());
    });

    Ok(())
}

#[cfg(target_os = "linux")]
fn serve_clipboard_daemon() -> Result<()> {
    let mut buffer = String::new();
    io::stdin().read_to_string(&mut buffer)?;

    let mut clipboard = Clipboard::new().context("Failed to initialize clipboard in daemon")?;
    clipboard
        .set_text(buffer)
        .context("Failed to set text in clipboard daemon")?;

    // Keep the daemon alive long enough for the content to be pasted.
    // A more robust solution might use a socket or D-Bus, but this is a simple, effective approach.
    std::thread::sleep(std::time::Duration::from_secs(60));

    Ok(())
}