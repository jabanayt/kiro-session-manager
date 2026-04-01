//! Centralised colour and formatting helpers.
//!
//! All CLI output styling goes through this module for consistency.

/// ANSI escape codes.
pub(crate) mod ansi {
    pub const RESET: &str = "\x1b[0m";
    pub const BOLD: &str = "\x1b[1m";
    pub const DIM: &str = "\x1b[90m";
    pub const GREEN: &str = "\x1b[32m";
    pub const YELLOW: &str = "\x1b[33m";
    pub const BLUE: &str = "\x1b[34m";
    pub const CYAN: &str = "\x1b[36m";
    pub const LIGHT_BLUE: &str = "\x1b[94m";
    pub const INVERSE: &str = "\x1b[1;7m";
    pub const RED: &str = "\x1b[31m";
}

/// Format index number: bold white `[0]`
pub fn index(n: usize) -> String {
    format!("{}[{}]{}", ansi::BOLD, n, ansi::RESET)
}

/// Format indexed marker: green `[i]`
pub fn indexed_marker() -> String {
    format!("{}[i]{}", ansi::GREEN, ansi::RESET)
}

/// Format session/archive name: plain white
pub fn name(s: &str) -> String {
    s.to_string()
}

/// Format chain link: light blue `↳ [N]`
pub fn chain_link(parent_idx: usize) -> String {
    format!("{}↳ [{}]{}", ansi::LIGHT_BLUE, parent_idx, ansi::RESET)
}

/// Format time: dim grey
pub fn time(s: &str) -> String {
    format!("{}{}{}", ansi::DIM, s, ansi::RESET)
}

/// Format message count: yellow
pub fn msg_count(s: &str) -> String {
    format!("{}{}{}", ansi::YELLOW, s, ansi::RESET)
}

/// Format tags: cyan, comma-separated
pub fn tags(tags: &[String]) -> String {
    if tags.is_empty() {
        String::new()
    } else {
        format!("{}{}{}", ansi::CYAN, tags.join(", "), ansi::RESET)
    }
}

/// Format pruned marker: yellow `[pruned]`
pub fn pruned_marker() -> String {
    format!("{}[pruned]{}", ansi::YELLOW, ansi::RESET)
}

/// Format legend/footer: dim grey
pub fn legend(s: &str) -> String {
    format!("{}{}{}", ansi::DIM, s, ansi::RESET)
}

/// Format success message: green checkmark
pub fn success(msg: &str) -> String {
    format!("{}✓{} {}", ansi::GREEN, ansi::RESET, msg)
}

/// Format warning message: yellow warning
pub fn warning(msg: &str) -> String {
    format!("{}⚠{} {}", ansi::YELLOW, ansi::RESET, msg)
}

/// Format user label: green
pub fn user_label() -> String {
    format!("{}User:{}", ansi::GREEN, ansi::RESET)
}

/// Format assistant label: blue
pub fn assistant_label() -> String {
    format!("{}Assistant:{}", ansi::BLUE, ansi::RESET)
}

/// Format tools label: yellow
pub fn tools_label() -> String {
    format!("{}Tools:{}", ansi::YELLOW, ansi::RESET)
}

/// Highlight search match: inverse
pub fn highlight(s: &str) -> String {
    s.replace(">>>", ansi::INVERSE).replace("<<<", ansi::RESET)
}

/// Format separator line: dim grey
pub fn separator(width: usize) -> String {
    format!("{}{}{}", ansi::DIM, "─".repeat(width), ansi::RESET)
}

/// Format a hint/command: green
pub fn hint(s: &str) -> String {
    format!("{}{}{}", ansi::GREEN, s, ansi::RESET)
}

/// Format bold text
pub fn bold(s: &str) -> String {
    format!("{}{}{}", ansi::BOLD, s, ansi::RESET)
}
