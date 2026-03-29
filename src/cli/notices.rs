//! Reusable notice renderer for contextual messages above command output.

use crate::cli::styles::ansi;

#[derive(Debug)]
pub enum NoticeLevel {
    Success,
    Info,
    Warning,
    Error,
}

#[derive(Debug)]
pub struct Notice {
    pub level: NoticeLevel,
    pub header: String,
    pub lines: Vec<String>,
}

impl Notice {
    pub fn success(header: &str) -> Self {
        Self {
            level: NoticeLevel::Success,
            header: header.to_string(),
            lines: vec![],
        }
    }

    pub fn info(header: &str) -> Self {
        Self {
            level: NoticeLevel::Info,
            header: header.to_string(),
            lines: vec![],
        }
    }

    pub fn warning(header: &str, lines: Vec<String>) -> Self {
        Self {
            level: NoticeLevel::Warning,
            header: header.to_string(),
            lines,
        }
    }

    pub fn error(header: &str, lines: Vec<String>) -> Self {
        Self {
            level: NoticeLevel::Error,
            header: header.to_string(),
            lines,
        }
    }
}

/// Strip ANSI escape codes from a string for width measurement.
fn strip_ansi(s: &str) -> String {
    let mut result = String::new();
    let mut in_escape = false;
    for c in s.chars() {
        if c == '\x1b' {
            in_escape = true;
        } else if in_escape {
            if c.is_ascii_alphabetic() {
                in_escape = false;
            }
        } else {
            result.push(c);
        }
    }
    result
}

/// Get the colour codes for a notice level.
fn level_colour(level: &NoticeLevel) -> &'static str {
    match level {
        NoticeLevel::Success => ansi::GREEN,
        NoticeLevel::Info => ansi::BLUE,
        NoticeLevel::Warning => ansi::YELLOW,
        NoticeLevel::Error => ansi::RED,
    }
}

/// Get the symbol for a notice level.
fn level_symbol(level: &NoticeLevel) -> &'static str {
    match level {
        NoticeLevel::Success => "✓",
        NoticeLevel::Info => "ℹ",
        NoticeLevel::Warning => "⚠",
        NoticeLevel::Error => "✗",
    }
}

/// Render a block of notices with coloured sidebar.
/// Returns empty string if notices is empty.
pub fn render_notices(notices: &[Notice]) -> String {
    if notices.is_empty() {
        return String::new();
    }

    let mut output = String::new();

    // Calculate max content width across all notices (strip ANSI for measurement)
    let mut max_width: usize = 0;
    for notice in notices {
        let header_plain = format!("{} {}", level_symbol(&notice.level), &notice.header);
        max_width = max_width.max(header_plain.len());
        for line in &notice.lines {
            let line_plain = format!("  {}", strip_ansi(line));
            max_width = max_width.max(line_plain.len());
        }
    }

    // Top border (colour of first notice)
    let first_colour = level_colour(&notices[0].level);
    output.push_str(&format!(
        "  {}┌{}{}\n",
        first_colour,
        "─".repeat(max_width),
        ansi::RESET
    ));

    for (i, notice) in notices.iter().enumerate() {
        let colour = level_colour(&notice.level);
        let symbol = level_symbol(&notice.level);

        // Blank separator line between notices (colour of current notice)
        if i > 0 {
            output.push_str(&format!("  {}│{}\n", colour, ansi::RESET));
        }

        // Header line
        output.push_str(&format!(
            "  {}{} {}{}\n",
            colour,
            symbol,
            notice.header,
            ansi::RESET
        ));

        // Detail lines with sidebar
        for line in &notice.lines {
            output.push_str(&format!("  {}│{} {}\n", colour, ansi::RESET, line));
        }
    }

    // Bottom border (colour of last notice)
    let last_colour = level_colour(&notices[notices.len() - 1].level);
    output.push_str(&format!(
        "  {}└{}{}\n",
        last_colour,
        "─".repeat(max_width),
        ansi::RESET
    ));

    output
}
