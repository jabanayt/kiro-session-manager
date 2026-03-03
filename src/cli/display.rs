use std::collections::HashMap;

use crate::models::{Archive, Chunk, SearchResult, Session, SessionMetadata};

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

/// Indent every line of `text` by `spaces` spaces.
fn indent_content(text: &str, spaces: usize) -> String {
    let pad = " ".repeat(spaces);
    text.lines()
        .map(|line| format!("{}{}", pad, line))
        .collect::<Vec<_>>()
        .join("\n")
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
        && !meta.tags.is_empty()
    {
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
        && let Some(parent_idx) = sessions.iter().position(|s| &s.id == parent_id)
    {
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

/// Format a search result for display.
///
/// Shows: archive name, exchange index, and content snippets.
pub fn format_search_result(result: &SearchResult, index: usize) -> String {
    let highlight_on = "\x1b[1;7m";
    let highlight_off = "\x1b[0m";

    let user_snippet = result
        .user_snippet
        .replace(">>>", highlight_on)
        .replace("<<<", highlight_off)
        .replace('\n', "\n    ");
    let assistant_snippet = result
        .assistant_snippet
        .replace(">>>", highlight_on)
        .replace("<<<", highlight_off)
        .replace('\n', "\n    ");

    let mut output = format!(
        "\x1b[90m──────────────────────────────────────── [{}] ────\x1b[0m\n{} -- exchange #{}\n    \x1b[32mUser:\x1b[0m {}\n    \x1b[34mAssistant:\x1b[0m {}",
        index, result.archive_name, result.exchange_index, user_snippet, assistant_snippet
    );

    if let Some(tool_snippet) = &result.tool_snippet {
        let tool_highlighted = tool_snippet
            .replace(">>>", highlight_on)
            .replace("<<<", highlight_off)
            .replace('\n', "\n    ");
        output.push_str(&format!("\n    \x1b[33mTools:\x1b[0m {}", tool_highlighted));
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

/// Format an expanded exchange as a string (for pager).
pub fn format_expanded_exchange(chunk: &Chunk, archive_name: &str) -> String {
    let mut output = format!(
        "\x1b[90m──── \x1b[0m{} -- exchange #{}\x1b[90m ────\x1b[0m\n",
        archive_name, chunk.exchange_index
    );
    output.push_str(&format!("\n\x1b[32mUser:\x1b[0m\n{}\n", indent_content(&chunk.user_content, 4)));
    output.push_str(&format!("\n\x1b[34mAssistant:\x1b[0m\n{}\n", indent_content(&chunk.assistant_content, 4)));

    if let Some(tool_summary) = &chunk.tool_summary {
        output.push_str(&format!("\n\x1b[33mTools:\x1b[0m\n{}\n", indent_content(tool_summary, 4)));
    }

    output.push('\n');
    output
}

/// Format an archive for the list-archives display.
pub fn format_archive_list_entry(archive: &Archive) -> String {
    let time_ago = format_time_ago(archive.archived_at);
    let msg_count = format_msg_count(archive.message_count);

    let mut output = format!(
        "\x1b[1m{}\x1b[0m | \x1b[90m{} | archived {}\x1b[0m",
        archive.name, msg_count, time_ago
    );

    if !archive.tags.is_empty() {
        let tags: Vec<&str> = archive.tags.iter().map(|s| s.as_str()).collect();
        output.push_str(&format!("\n  Tags: {}", tags.join(", ")));
    }

    if archive.pruned {
        output.push_str("\n  \x1b[33m[pruned]\x1b[0m");
    }

    output
}

/// Print the full list of archives.
pub fn print_archive_list(archives: &[Archive]) {
    if archives.is_empty() {
        println!("No archives found.");
        return;
    }

    println!("\nArchived Sessions:\n");
    for archive in archives {
        println!("{}", format_archive_list_entry(archive));
        println!();
    }
}

/// Format a full archived conversation as a string (for pager).
pub fn format_full_archive(archive: &Archive, chunks: &[Chunk]) -> String {
    let session_date = format_time_ago(archive.session_created_at);
    let archived_date = format_time_ago(archive.archived_at);
    let msg_count = format_msg_count(archive.message_count);

    let mut output = format!(
        "\n\x1b[1m{}\x1b[0m | {} | session {} | archived {}\n",
        archive.name, msg_count, session_date, archived_date
    );

    if archive.pruned {
        output.push_str("\x1b[33m[pruned]\x1b[0m\n");
    }

    if !archive.tags.is_empty() {
        let tags: Vec<&str> = archive.tags.iter().map(|s| s.as_str()).collect();
        output.push_str(&format!("Tags: {}\n", tags.join(", ")));
    }

    output.push('\n');

    let max_lines = 10;

    for chunk in chunks {
        output.push_str(&format!(
            "\x1b[90m────────────────────────────────────── [{}] ────\x1b[0m\n",
            chunk.exchange_index
        ));
        output.push_str(&format!("\n\x1b[32mUser:\x1b[0m\n{}\n", indent_content(&chunk.user_content, 4)));
        output.push_str("\n\x1b[34mAssistant:\x1b[0m\n");

        let lines: Vec<&str> = chunk.assistant_content.lines().collect();
        if lines.len() > max_lines {
            for line in &lines[..max_lines] {
                output.push_str(&format!("    {}\n", line));
            }
            output.push_str(&format!(
                "\x1b[90m    ... ({} more lines, use --exchange {} to view)\x1b[0m\n",
                lines.len() - max_lines,
                chunk.exchange_index
            ));
        } else {
            output.push_str(&indent_content(&chunk.assistant_content, 4));
            output.push('\n');
        }

        if let Some(tool_summary) = &chunk.tool_summary {
            output.push_str(&format!("\n\x1b[33mTools:\x1b[0m\n{}\n", indent_content(tool_summary, 4)));
        }

        output.push('\n');
    }

    output
}

/// Format a single exchange as a string (for pager).
pub fn format_single_exchange(archive: &Archive, chunk: &Chunk) -> String {
    let mut output = format!(
        "\n\x1b[90m────────────────────────────────────── [{}] ────\x1b[0m\n",
        chunk.exchange_index
    );
    output.push_str(&format!(
        "\x1b[1m{}\x1b[0m -- exchange #{}\n",
        archive.name, chunk.exchange_index
    ));
    output.push_str(&format!("\n\x1b[32mUser:\x1b[0m\n{}\n", indent_content(&chunk.user_content, 4)));
    output.push_str(&format!("\n\x1b[34mAssistant:\x1b[0m\n{}\n", indent_content(&chunk.assistant_content, 4)));

    if let Some(tool_summary) = &chunk.tool_summary {
        output.push_str(&format!("\n\x1b[33mTools:\x1b[0m\n{}\n", indent_content(tool_summary, 4)));
    }

    output.push('\n');
    output
}
