#[macro_export]
macro_rules! dbg_tty {
    ($($arg:tt)*) => {{
        use std::io::Write;
        #[cfg(unix)]   const TTY: &str = "/dev/tty";
        #[cfg(windows)]const TTY: &str = "CON";
        if let Ok(mut f) = std::fs::OpenOptions::new().append(true).open(TTY) {
            let _ = writeln!(f, $($arg)*);
        }
    }}
}
