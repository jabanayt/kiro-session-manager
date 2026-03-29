//! Display formatting for CLI output.

use std::collections::HashMap;
use unicode_width::UnicodeWidthStr;

use crate::cli::pager;
use crate::cli::styles;
use crate::models::{Archive, Chunk, SearchResult, Session, SessionMetadata};

/// Format a millisecond timestamp as compact relative time.
///
/// Returns: 2s, 14m, 19h, 1d, 2w, 3mo, 1y
pub fn format_time_compact(timestamp_ms: i64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;

    let diff_secs = (now - timestamp_ms) / 1000;

    if diff_secs < 60 {
        return format!("{}s", diff_secs);
    }
    let diff_mins = diff_secs / 60;
    if diff_mins < 60 {
        return format!("{}m", diff_mins);
    }
    let diff_hours = diff_mins / 60;
    if diff_hours < 24 {
        return format!("{}h", diff_hours);
    }
    let diff_days = diff_hours / 24;
    if diff_days < 7 {
        return format!("{}d", diff_days);
    }
    let diff_weeks = diff_days / 7;
    if diff_weeks < 4 {
        return format!("{}w", diff_weeks);
    }
    let diff_months = diff_days / 30;
    if diff_months < 12 {
        return format!("{}mo", diff_months);
    }
    let diff_years = diff_days / 365;
    format!("{}y", diff_years)
}

/// Format a millisecond timestamp as verbose relative time.
///
/// Used in archive viewing for more readable timestamps.
///
/// TODO(v0.3.0): Consider removing - could use format_time_compact everywhere.
pub fn format_time_ago(timestamp_ms: i64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;

    let diff_ms = now - timestamp_ms;
    let diff_secs = diff_ms / 1000;

    if diff_secs < 60 {
        return format!("{} seconds ago", diff_secs);
    }
    let diff_mins = diff_secs / 60;
    if diff_mins < 60 {
        return if diff_mins == 1 {
            "1 minute ago".to_string()
        } else {
            format!("{} minutes ago", diff_mins)
        };
    }
    let diff_hours = diff_mins / 60;
    if diff_hours < 24 {
        return if diff_hours == 1 {
            "1 hour ago".to_string()
        } else {
            format!("{} hours ago", diff_hours)
        };
    }
    let diff_days = diff_hours / 24;
    if diff_days == 1 {
        "1 day ago".to_string()
    } else {
        format!("{} days ago", diff_days)
    }
}

/// Format message count.
pub fn format_msg_count(count: u32) -> String {
    if count == 1 {
        "1 msg".to_string()
    } else {
        format!("{} msgs", count)
    }
}

/// Truncate string to max display width, adding "..." if truncated.
fn truncate_to_width(s: &str, max_width: usize) -> String {
    let width = UnicodeWidthStr::width(s);
    if width <= max_width {
        return s.to_string();
    }

    let mut result = String::new();
    let mut current_width = 0;
    let suffix = "...";
    let suffix_width = 3;
    let target_width = max_width.saturating_sub(suffix_width);

    for c in s.chars() {
        let char_width = UnicodeWidthStr::width(c.to_string().as_str());
        if current_width + char_width > target_width {
            break;
        }
        result.push(c);
        current_width += char_width;
    }

    result.push_str(suffix);
    result
}

/// Pad string to exact display width.
#[allow(dead_code)]
fn pad_to_width(s: &str, width: usize) -> String {
    let current_width = UnicodeWidthStr::width(s);
    if current_width >= width {
        s.to_string()
    } else {
        format!("{}{}", s, " ".repeat(width - current_width))
    }
}

/// Get display name for a session (name or preview).
fn get_display_name(session: &Session, metadata: &HashMap<String, SessionMetadata>) -> String {
    metadata
        .get(&session.id)
        .and_then(|m| m.name.clone())
        .unwrap_or_else(|| session.preview.clone())
}

/// Get tags for a session.
fn get_tags(session_id: &str, metadata: &HashMap<String, SessionMetadata>) -> Vec<String> {
    metadata
        .get(session_id)
        .map(|m| {
            let mut tags: Vec<String> = m.tags.iter().cloned().collect();
            tags.sort();
            tags
        })
        .unwrap_or_default()
}

/// Check if session is indexed.
fn is_indexed(session_id: &str, indexed_ids: &[String]) -> bool {
    indexed_ids.contains(&session_id.to_string())
}

/// Get parent index if session has a parent.
fn get_parent_index(
    session_id: &str,
    metadata: &HashMap<String, SessionMetadata>,
    sessions: &[Session],
) -> Option<usize> {
    metadata
        .get(session_id)
        .and_then(|m| m.parent_session_id.as_ref())
        .and_then(|parent_id| sessions.iter().position(|s| &s.id == parent_id))
}

/// Print the session list with new column-aligned format.
///
/// Format:
/// [0]  [i]  Session name here...              2s ago     9 msgs   tag1, tag2
/// [1]       Another session                  14m ago   104 msgs   
pub fn print_session_list(
    sessions: &[Session],
    metadata: &HashMap<String, SessionMetadata>,
    visible_indices: &[usize],
    indexed_session_ids: &[String],
    show_parents: bool,
) {
    if visible_indices.is_empty() {
        println!("No sessions found.");
        return;
    }

    const TIME_WIDTH: usize = 8;
    const MSG_WIDTH: usize = 9;

    // Calculate index column width based on largest index
    let max_idx = visible_indices.iter().max().unwrap_or(&0);
    let idx_width = format!("[{}]", max_idx).len();

    // Dynamic name width based on terminal width
    // Fixed columns: indexed marker (5) + spacing (2) + TIME_WIDTH (8) + spacing (2) + MSG_WIDTH (9) + spacing (2) = 28
    let name_width = pager::terminal_size()
        .map(|(w, _)| w.saturating_sub(idx_width).saturating_sub(28).max(20))
        .unwrap_or(35);

    println!();
    for &idx in visible_indices {
        let session = &sessions[idx];
        let name = get_display_name(session, metadata);
        let tags = get_tags(&session.id, metadata);
        let indexed = is_indexed(&session.id, indexed_session_ids);
        let parent_idx = get_parent_index(&session.id, metadata, sessions);

        // Build display name with chain link (only when not showing parent tree)
        let display_name = if let Some(pidx) = parent_idx
            && !show_parents
        {
            let chain = format!(" {}", styles::chain_link(pidx));
            let chain_plain = format!(" ↳ [{}]", pidx);
            let available = name_width.saturating_sub(UnicodeWidthStr::width(chain_plain.as_str()));
            format!("{}{}", truncate_to_width(&name, available), chain)
        } else {
            truncate_to_width(&name, name_width)
        };

        // Calculate plain width for padding
        let plain_name = if let Some(pidx) = parent_idx
            && !show_parents
        {
            let chain_plain = format!(" ↳ [{}]", pidx);
            format!(
                "{}{}",
                truncate_to_width(
                    &name,
                    name_width.saturating_sub(UnicodeWidthStr::width(chain_plain.as_str()))
                ),
                chain_plain
            )
        } else {
            truncate_to_width(&name, name_width)
        };
        let name_padded = format!(
            "{}{}",
            display_name,
            " ".repeat(name_width.saturating_sub(UnicodeWidthStr::width(plain_name.as_str())))
        );

        let time_str = format_time_compact(session.updated_at);
        let time_padded = format!("{:>width$}", time_str, width = TIME_WIDTH);

        let msg_str = format_msg_count(session.msg_count);
        let msg_padded = format!("{:>width$}", msg_str, width = MSG_WIDTH);

        let idx_str = styles::index(idx);
        let idx_plain = format!("[{}]", idx);
        let idx_padded = format!(
            "{}{}",
            idx_str,
            " ".repeat(idx_width.saturating_sub(idx_plain.len()))
        );
        let indexed_str = if indexed {
            format!("  {}", styles::indexed_marker())
        } else {
            "     ".to_string()
        };

        let tags_str = if tags.is_empty() {
            String::new()
        } else {
            format!("   {}", styles::tags(&tags))
        };

        println!(
            "{}{}  {}  {}  {}{}",
            idx_padded,
            indexed_str,
            name_padded,
            styles::time(&time_padded),
            styles::msg_count(&msg_padded),
            tags_str
        );

        // Show parent chain if requested
        if show_parents {
            let mut current_id = session.id.clone();
            let mut depth = 1;
            while let Some(meta) = metadata.get(&current_id) {
                if let Some(parent_id) = &meta.parent_session_id {
                    if let Some(pidx) = sessions.iter().position(|s| &s.id == parent_id) {
                        let parent = &sessions[pidx];
                        let parent_name = get_display_name(parent, metadata);
                        let parent_time = format_time_compact(parent.updated_at);
                        let indent = "    ".repeat(depth);
                        println!(
                            "{}{} {} ({})",
                            indent,
                            styles::chain_link(pidx),
                            parent_name,
                            styles::time(&parent_time)
                        );
                        current_id = parent_id.clone();
                        depth += 1;
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }
        }
    }

    // Legend
    if indexed_session_ids
        .iter()
        .any(|id| visible_indices.iter().any(|&idx| &sessions[idx].id == id))
    {
        println!();
        println!(
            "{}",
            styles::legend("[i] = indexed (searchable via ksm search)")
        );
    }
}

/// Print the archive list with new column-aligned format.
///
/// Format:
/// [0]  steering-archival-guidelines...     1d ago    33 msgs   steering, documentation
pub fn print_archive_list(archives: &[Archive]) {
    if archives.is_empty() {
        println!("No archives found.");
        return;
    }

    const TIME_WIDTH: usize = 8;
    const MSG_WIDTH: usize = 9;

    // Calculate index column width based on largest index
    let max_idx = archives.len().saturating_sub(1);
    let idx_width = format!("[{}]", max_idx).len();

    // Dynamic name width based on terminal width
    // Fixed columns: spacing (2) + TIME_WIDTH (8) + spacing (2) + MSG_WIDTH (9) + spacing (2) = 23
    let name_width = pager::terminal_size()
        .map(|(w, _)| w.saturating_sub(idx_width).saturating_sub(23).max(20))
        .unwrap_or(35);

    println!();
    for (idx, archive) in archives.iter().enumerate() {
        let name_with_pruned = if archive.pruned {
            format!("{} {}", archive.name, styles::pruned_marker())
        } else {
            archive.name.clone()
        };

        // For width calculation, use plain text
        let name_plain = if archive.pruned {
            format!("{} [pruned]", archive.name)
        } else {
            archive.name.clone()
        };

        let name_truncated = truncate_to_width(&name_with_pruned, name_width);
        let name_plain_truncated = truncate_to_width(&name_plain, name_width);
        let name_padded = format!(
            "{}{}",
            name_truncated,
            " ".repeat(
                name_width.saturating_sub(UnicodeWidthStr::width(name_plain_truncated.as_str()))
            )
        );

        let time_str = format_time_compact(archive.archived_at);
        let time_padded = format!("{:>width$}", time_str, width = TIME_WIDTH);

        let msg_str = format_msg_count(archive.message_count);
        let msg_padded = format!("{:>width$}", msg_str, width = MSG_WIDTH);

        let tags_str = if archive.tags.is_empty() {
            String::new()
        } else {
            format!("   {}", styles::tags(&archive.tags))
        };

        println!(
            "{}  {}  {}  {}{}",
            styles::index(idx),
            name_padded,
            styles::time(&time_padded),
            styles::msg_count(&msg_padded),
            tags_str
        );
    }
}

// ========== Search Result Formatting (updated with styles) ==========

/// Format a search result for display.
pub fn format_search_result(result: &SearchResult, index: usize) -> String {
    let user_snippet = styles::highlight(&result.user_snippet).replace('\n', "\n    ");
    let assistant_snippet = styles::highlight(&result.assistant_snippet).replace('\n', "\n    ");

    let mut output = format!(
        "{} [{}] {}\n{} -- exchange #{}\n    {} {}\n    {} {}",
        styles::separator(40),
        index,
        styles::separator(4),
        result.archive_name,
        result.exchange_index,
        styles::user_label(),
        user_snippet,
        styles::assistant_label(),
        assistant_snippet
    );

    if let Some(tool_snippet) = &result.tool_snippet {
        let tool_highlighted = styles::highlight(tool_snippet).replace('\n', "\n    ");
        output.push_str(&format!(
            "\n    {} {}",
            styles::tools_label(),
            tool_highlighted
        ));
    }

    output
}

/// Format search results as a single string (for pager).
pub fn format_search_results(results: &[SearchResult]) -> String {
    if results.is_empty() {
        return "No results found.\n".to_string();
    }

    let mut output = String::new();
    for (i, result) in results.iter().enumerate() {
        output.push_str(&format_search_result(result, i));
        output.push_str("\n\n");
    }
    output.push_str("Use --expand N to show the full exchange for result N.\n");
    output
}

// ========== Archive Viewing (keep existing, update labels) ==========

fn indent_content(text: &str, spaces: usize) -> String {
    let pad = " ".repeat(spaces);
    text.lines()
        .map(|line| format!("{}{}", pad, line))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Format an expanded exchange as a string (for pager).
pub fn format_expanded_exchange(chunk: &Chunk, archive_name: &str) -> String {
    let mut output = format!(
        "{} {} -- exchange #{} {}\n",
        styles::separator(4),
        archive_name,
        chunk.exchange_index,
        styles::separator(4)
    );
    output.push_str(&format!(
        "\n{}\n{}\n",
        styles::user_label(),
        indent_content(&chunk.user_content, 4)
    ));
    output.push_str(&format!(
        "\n{}\n{}\n",
        styles::assistant_label(),
        indent_content(&chunk.assistant_content, 4)
    ));

    if let Some(tool_summary) = &chunk.tool_summary {
        output.push_str(&format!(
            "\n{}\n{}\n",
            styles::tools_label(),
            indent_content(tool_summary, 4)
        ));
    }

    output.push('\n');
    output
}

/// Format a full archived conversation as a string (for pager).
pub fn format_full_archive(archive: &Archive, chunks: &[Chunk]) -> String {
    let session_date = format_time_ago(archive.session_created_at);
    let archived_date = format_time_ago(archive.archived_at);
    let msg_count = format_msg_count(archive.message_count);

    let mut output = format!(
        "\n{} | {} | session {} | archived {}\n",
        styles::name(&archive.name),
        styles::msg_count(&msg_count),
        styles::time(&session_date),
        styles::time(&archived_date)
    );

    if archive.pruned {
        output.push_str(&format!("{}\n", styles::pruned_marker()));
    }

    if !archive.tags.is_empty() {
        output.push_str(&format!("Tags: {}\n", styles::tags(&archive.tags)));
    }

    output.push('\n');

    let max_lines = 10;

    for chunk in chunks {
        output.push_str(&format!(
            "{} [{}] {}\n",
            styles::separator(38),
            chunk.exchange_index,
            styles::separator(4)
        ));
        output.push_str(&format!(
            "\n{}\n{}\n",
            styles::user_label(),
            indent_content(&chunk.user_content, 4)
        ));
        output.push_str(&format!("\n{}\n", styles::assistant_label()));

        let lines: Vec<&str> = chunk.assistant_content.lines().collect();
        if lines.len() > max_lines {
            for line in &lines[..max_lines] {
                output.push_str(&format!("    {}\n", line));
            }
            output.push_str(&format!(
                "{}    ... ({} more lines, use --exchange {} to view){}\n",
                "\x1b[90m",
                lines.len() - max_lines,
                chunk.exchange_index,
                "\x1b[0m"
            ));
        } else {
            output.push_str(&indent_content(&chunk.assistant_content, 4));
            output.push('\n');
        }

        if let Some(tool_summary) = &chunk.tool_summary {
            output.push_str(&format!(
                "\n{}\n{}\n",
                styles::tools_label(),
                indent_content(tool_summary, 4)
            ));
        }

        output.push('\n');
    }

    output
}

/// Format a single exchange as a string (for pager).
pub fn format_single_exchange(archive: &Archive, chunk: &Chunk) -> String {
    let mut output = format!(
        "\n{} [{}] {}\n",
        styles::separator(38),
        chunk.exchange_index,
        styles::separator(4)
    );
    output.push_str(&format!(
        "{} -- exchange #{}\n",
        styles::name(&archive.name),
        chunk.exchange_index
    ));
    output.push_str(&format!(
        "\n{}\n{}\n",
        styles::user_label(),
        indent_content(&chunk.user_content, 4)
    ));
    output.push_str(&format!(
        "\n{}\n{}\n",
        styles::assistant_label(),
        indent_content(&chunk.assistant_content, 4)
    ));

    if let Some(tool_summary) = &chunk.tool_summary {
        output.push_str(&format!(
            "\n{}\n{}\n",
            styles::tools_label(),
            indent_content(tool_summary, 4)
        ));
    }

    output.push('\n');
    output
}
