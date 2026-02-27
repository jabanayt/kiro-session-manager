use std::collections::HashMap;

use crate::models::{Session, SessionMetadata};

/// Format a millisecond timestamp as a relative time string.
///
/// Logic from current database.rs lines 86-117.
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

/// Format message count as "X msgs" or "1 msg".
///
/// Replaces current database.rs format_msg_count (which operated on ConversationData).
pub fn format_msg_count(count: u32) -> String {
    if count == 1 {
        "1 msg".to_string()
    } else {
        format!("{} msgs", count)
    }
}

/// Format a session's display line (tags + name/preview + parent indicator).
///
/// Logic from current commands/list.rs lines 24-72.
pub fn format_session_display(
    session: &Session,
    metadata: &HashMap<String, SessionMetadata>,
    sessions: &[Session],
    include_original: bool,
    show_parent_inline: bool,
) -> String {
    let meta = metadata.get(&session.id);
    let mut display = String::new();

    // Add tags if present
    if let Some(meta) = meta
        && !meta.tags.is_empty() {
            let mut tags: Vec<_> = meta.tags.iter().collect();
            tags.sort();
            for tag in tags {
                display.push_str(&format!("[{}] ", tag));
            }
        }

    // Add name or preview
    if let Some(meta) = meta {
        if let Some(name) = &meta.name {
            display.push_str(name);
            if include_original {
                display.push_str(&format!(" ({})", session.preview));
            }
        } else {
            display.push_str(&session.preview);
        }
    } else {
        display.push_str(&session.preview);
    }

    // Add parent indicator
    if show_parent_inline
        && let Some(meta) = meta
            && let Some(parent_id) = &meta.parent_session_id
                && let Some(parent_idx) = sessions.iter().position(|s| &s.id == parent_id) {
                    display.push_str(&format!(" \x1b[36m↳ from [{}]\x1b[0m", parent_idx));
                }

    display
}

/// Print the filtered session list to stdout.
///
/// `visible_indices` are the indices into `sessions` that should be displayed
/// (pre-computed by the caller via `sessions::visible_session_indices()`).
/// This keeps display.rs as a pure formatting module with no services dependency.
///
/// Logic from current commands/list.rs lines 75-122.
pub fn print_session_list(
    sessions: &[Session],
    metadata: &HashMap<String, SessionMetadata>,
    visible_indices: &[usize],
    show_parents: bool,
) {
    if visible_indices.is_empty() {
        println!("No sessions found.");
        return;
    }

    println!("\nKiro Chat Sessions:\n");
    for &idx in visible_indices {
        let session = &sessions[idx];
        let time_ago = format_time_ago(session.updated_at);
        let msg_count = format_msg_count(session.msg_count);

        if show_parents {
            let display = format_session_display(session, metadata, sessions, false, false);
            println!("[{}] {} | {} | {}", idx, time_ago, msg_count, display);

            // Show parent chain with details and indentation
            let mut current_id = session.id.clone();
            let mut depth = 1;
            while let Some(meta) = metadata.get(&current_id) {
                if let Some(parent_id) = &meta.parent_session_id {
                    if let Some(parent_idx) = sessions.iter().position(|s| &s.id == parent_id) {
                        let parent = &sessions[parent_idx];
                        let parent_display =
                            format_session_display(parent, metadata, sessions, false, false);
                        let parent_time = format_time_ago(parent.updated_at);
                        let indent = "    ".repeat(depth);
                        println!(
                            "{}\x1b[36m↳ from [{}]\x1b[0m {} ({})",
                            indent, parent_idx, parent_display, parent_time
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
        } else {
            let display = format_session_display(session, metadata, sessions, false, true);
            println!("[{}] {} | {} | {}", idx, time_ago, msg_count, display);
        }
    }
}
