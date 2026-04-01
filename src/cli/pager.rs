//! Pager support for long output.
//!
//! Pipes content through the system pager (e.g. `less`) when output
//! exceeds the terminal height. Falls back to direct stdout if the
//! pager is unavailable or stdout is not a TTY.

use std::io::{self, IsTerminal, Write};
use std::process::{Command, Stdio};

/// Get terminal size (width, height) via ioctl, or None if unavailable.
pub fn terminal_size() -> Option<(usize, usize)> {
    unsafe {
        let mut winsize: libc::winsize = std::mem::zeroed();
        if libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut winsize) == 0
            && winsize.ws_row > 0
        {
            Some((winsize.ws_col as usize, winsize.ws_row as usize))
        } else {
            None
        }
    }
}

/// Write content through a pager if it exceeds terminal height.
///
/// Falls back to direct stdout if:
/// - stdout is not a TTY (e.g. piped)
/// - terminal height cannot be determined
/// - content fits within the terminal
/// - pager process fails to spawn
pub fn paged_output(content: &str) -> io::Result<()> {
    if !io::stdout().is_terminal() {
        return io::stdout().write_all(content.as_bytes());
    }

    let height = match terminal_size().map(|(_, h)| h) {
        Some(h) => h,
        None => return io::stdout().write_all(content.as_bytes()),
    };

    let line_count = content.lines().count();
    if line_count <= height.saturating_sub(2) {
        return io::stdout().write_all(content.as_bytes());
    }

    let pager = std::env::var("PAGER").unwrap_or_else(|_| "less".to_string());
    let mut args = vec![];
    if pager.ends_with("less") {
        args.push("-R");
    }

    let mut child = match Command::new(&pager)
        .args(&args)
        .stdin(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(_) => return io::stdout().write_all(content.as_bytes()),
    };

    if let Some(mut stdin) = child.stdin.take() {
        // Ignore broken pipe (user quit pager early)
        let _ = stdin.write_all(content.as_bytes());
    }

    let _ = child.wait();
    Ok(())
}
