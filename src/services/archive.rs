//! Archive service: extract, clean, chunk, save, search, delete.
//!
//! This is the core business logic for the archive feature. It reads
//! conversations via SessionSource, processes them into clean chunks,
//! and saves/searches via ArchiveStore.

use crate::data::{KsmDatabase, SessionSource};
use crate::error::{KsmError, Result};
use crate::models::{
    Archive, ArchiveResult, ArchiveStatus, AssistantContent, Chunk, ConversationData,
    DeleteArchiveResult, NewArchive, NewChunk, SearchQuery, SearchResult, Session,
    ShowArchiveResult, SourceType, ToolCall, ToolResultContent, UserContent,
};
use crate::services::metadata::validate_tags;

// --- Archive operation ---

/// Archive a session: extract conversation, clean, chunk, save, then delete.
pub fn archive_session(
    session: &Session,
    name: &str,
    tags: Vec<String>,
    directory: &str,
    source: &dyn SessionSource,
    db: &KsmDatabase,
) -> Result<ArchiveResult> {
    let tags = validate_tags(&tags)?;
    let session_id = &session.id;
    let session_created_at = session.created_at;
    let source_type = session.source_type;
    // Check if already indexed - if so, convert to archive
    if let Some(status) = db.get_archive_status_for_source(session_id, source_type)? {
        match status {
            ArchiveStatus::Indexed { archive_id, .. } => {
                // Convert indexed to archived
                db.set_indexed(archive_id, false)?;
                source.delete_session(session_id, source_type)?;
                let archive = db.get_archive_by_id(archive_id)?;
                return Ok(ArchiveResult {
                    archive_name: archive.name,
                    chunk_count: 0, // Already indexed
                    message_count: archive.message_count,
                    pruned: archive.pruned,
                });
            }
            ArchiveStatus::Archived { name, .. } => {
                return Err(KsmError::AlreadyArchived(name));
            }
        }
    }

    let conversation = source.get_conversation(session_id, source_type)?;
    let pruned = is_pruned(&conversation);
    let chunks = extract_chunks(&conversation);

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time before UNIX epoch")
        .as_millis() as i64;

    let new_archive = NewArchive {
        session_id: session_id.to_string(),
        name: name.to_string(),
        directory: directory.to_string(),
        message_count: conversation.history.len() as u32,
        session_created_at,
        archived_at: now,
        tags,
        pruned,
        source_type,
    };

    db.save_archive(&new_archive, &chunks, false)?; // is_indexed = false

    // Delete session from Kiro
    source.delete_session(session_id, source_type)?;

    Ok(ArchiveResult {
        archive_name: name.to_string(),
        chunk_count: chunks.len(),
        message_count: new_archive.message_count,
        pruned,
    })
}

// --- Search operation ---

/// Search archived sessions for the given project directory.
pub fn search_archives(
    query_text: &str,
    limit: u32,
    directory: &str,
    db: &KsmDatabase,
) -> Result<Vec<SearchResult>> {
    let query = SearchQuery {
        query: sanitize_fts_query(query_text),
        directory: directory.to_string(),
        limit,
    };
    db.search(&query)
}

/// Sanitize a query string for FTS5.
///
/// Strips punctuation that causes syntax errors. These characters are not
/// indexed by FTS5 anyway, so removing them doesn't affect search results.
/// Hyphens are replaced with spaces to handle hyphenated words.
fn sanitize_fts_query(query: &str) -> String {
    let strip: &[char] = &[
        '.', ',', '\'', ';', ':', '!', '?', '(', ')', '[', ']', '{', '}', '+',
    ];

    query
        .chars()
        .filter(|c| !strip.contains(c))
        .map(|c| if c == '-' { ' ' } else { c })
        .collect()
}

/// Get the full exchange content for a specific search result.
///
/// Used by --expand to show the complete exchange instead of just a snippet.
pub fn get_expanded_result(
    archive_name: &str,
    exchange_index: i32,
    directory: &str,
    db: &KsmDatabase,
) -> Result<Chunk> {
    let archive = db.get_archive(archive_name, directory)?;
    let chunks = db.get_chunks(archive.id)?;

    chunks
        .into_iter()
        .find(|c| c.exchange_index == exchange_index)
        .ok_or_else(|| KsmError::ExchangeNotFound {
            index: exchange_index,
            archive: archive_name.to_string(),
        })
}

/// List all archives for the given project directory.
pub fn list_archives(directory: &str, db: &KsmDatabase) -> Result<Vec<Archive>> {
    db.list_archives(directory)
}

/// Get a full archived conversation for browsing.
pub fn show_archive(
    archive_name: &str,
    directory: &str,
    db: &KsmDatabase,
) -> Result<ShowArchiveResult> {
    let archive = db.get_archive(archive_name, directory)?;
    let chunks = db.get_chunks(archive.id)?;
    Ok(ShowArchiveResult { archive, chunks })
}

/// Get archive info for confirmation prompts.
pub fn get_archive_info(name: &str, directory: &str, db: &KsmDatabase) -> Result<Archive> {
    db.get_archive(name, directory)
}

/// Get archive info by index (from list-archives display order).
pub fn get_archive_by_index(index: usize, directory: &str, db: &KsmDatabase) -> Result<Archive> {
    let archives = db.list_archives(directory)?;
    if index >= archives.len() {
        return Err(KsmError::IndexOutOfRange {
            index,
            max: archives.len().saturating_sub(1),
        });
    }
    Ok(archives[index].clone())
}

/// Delete an archive and all its indexed content.
pub fn delete_archive(
    archive_name: &str,
    directory: &str,
    db: &KsmDatabase,
) -> Result<DeleteArchiveResult> {
    let archive = db.get_archive(archive_name, directory)?;
    db.delete_archive(archive.id)?;

    Ok(DeleteArchiveResult {
        archive_name: archive_name.to_string(),
        message_count: archive.message_count,
    })
}

/// Delete archive by index (from list-archives display order).
pub fn delete_archive_by_index(
    index: usize,
    directory: &str,
    db: &KsmDatabase,
) -> Result<DeleteArchiveResult> {
    let archives = db.list_archives(directory)?;

    if index >= archives.len() {
        return Err(KsmError::IndexOutOfRange {
            index,
            max: archives.len().saturating_sub(1),
        });
    }

    let archive = &archives[index];
    db.delete_archive(archive.id)?;

    Ok(DeleteArchiveResult {
        archive_name: archive.name.clone(),
        message_count: archive.message_count,
    })
}

// ========== Index Operations ==========

/// Index a session (add to search without deleting from Kiro).
pub fn index_session(
    session: &Session,
    name: &str,
    tags: Vec<String>,
    directory: &str,
    source: &dyn SessionSource,
    db: &KsmDatabase,
) -> Result<ArchiveResult> {
    let tags = validate_tags(&tags)?;
    let session_id = &session.id;
    let session_created_at = session.created_at;
    let source_type = session.source_type;
    // Check if already indexed/archived
    if let Some(status) = db.get_archive_status_for_source(session_id, source_type)? {
        let existing_name = match status {
            ArchiveStatus::Indexed { name, .. } => name,
            ArchiveStatus::Archived { name, .. } => name,
        };
        return Err(KsmError::AlreadyArchived(existing_name));
    }

    let conversation = source.get_conversation(session_id, source_type)?;
    let pruned = is_pruned(&conversation);
    let chunks = extract_chunks(&conversation);

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time before UNIX epoch")
        .as_millis() as i64;

    let new_archive = NewArchive {
        session_id: session_id.to_string(),
        name: name.to_string(),
        directory: directory.to_string(),
        message_count: conversation.history.len() as u32,
        session_created_at,
        archived_at: now,
        tags,
        pruned,
        source_type,
    };

    db.save_archive(&new_archive, &chunks, true)?; // is_indexed = true

    Ok(ArchiveResult {
        archive_name: name.to_string(),
        chunk_count: chunks.len(),
        message_count: new_archive.message_count,
        pruned,
    })
}

/// Reindex a specific session by session ID.
pub fn reindex_session(
    session_id: &str,
    source: &dyn SessionSource,
    db: &KsmDatabase,
    source_type: SourceType,
) -> Result<ReindexResult> {
    let status = db.get_archive_status_for_source(session_id, source_type)?;

    let (name, archive_id) = match status {
        Some(ArchiveStatus::Indexed {
            name, archive_id, ..
        }) => (name, archive_id),
        Some(ArchiveStatus::Archived { name, .. }) => {
            return Err(KsmError::CannotReindexArchived(name));
        }
        None => {
            return Err(KsmError::NotIndexed(session_id.to_string()));
        }
    };

    let archive = db.get_archive_by_id(archive_id)?;
    let old_count = archive.message_count;

    let conversation = source.get_conversation(session_id, source_type)?;
    let new_count = conversation.history.len() as u32;

    // Safety check
    if new_count < old_count {
        return Err(KsmError::SessionCompacted {
            old: old_count,
            new: new_count,
        });
    }

    let chunks = extract_chunks(&conversation);
    db.update_archive(archive_id, new_count, &chunks)?;

    Ok(ReindexResult {
        name,
        old_count,
        new_count,
        updated: new_count != old_count,
        error: None,
    })
}

/// Reindex all indexed sessions for a directory.
pub fn reindex_all(
    directory: &str,
    source: &dyn SessionSource,
    db: &KsmDatabase,
) -> Result<Vec<ReindexResult>> {
    let indexed = db.list_indexed(directory)?;
    let mut results = Vec::new();

    for archive in indexed {
        match reindex_session(&archive.session_id, source, db, archive.source_type) {
            Ok(result) => results.push(result),
            Err(e) => {
                results.push(ReindexResult {
                    name: archive.name,
                    old_count: archive.message_count,
                    new_count: archive.message_count,
                    updated: false,
                    error: Some(e.to_string()),
                });
            }
        }
    }

    Ok(results)
}

/// Remove index from a session (delete archive entry, session stays in Kiro).
pub fn unindex_session(
    session_id: &str,
    db: &KsmDatabase,
    source_type: SourceType,
) -> Result<UnindexResult> {
    match db.get_archive_status_for_source(session_id, source_type)? {
        Some(ArchiveStatus::Indexed {
            name, archive_id, ..
        }) => {
            db.delete_archive(archive_id)?;
            Ok(UnindexResult { name })
        }
        _ => Err(KsmError::NotIndexed(session_id.to_string())),
    }
}

/// Result of an unindex operation.
#[derive(Debug)]
pub struct UnindexResult {
    pub name: String,
}

/// Result of a reindex operation.
#[derive(Debug)]
pub struct ReindexResult {
    pub name: String,
    pub old_count: u32,
    pub new_count: u32,
    pub updated: bool,
    pub error: Option<String>,
}

/// Extract chunks from a conversation (public for auto-reindex).
pub fn extract_chunks_from_conversation(conversation: &ConversationData) -> Vec<NewChunk> {
    extract_chunks(conversation)
}

/// Result of pending reindex check on startup.
#[derive(Debug)]
pub struct PendingReindexResult {
    pub session_name: Option<String>,
    pub updated: bool,
    pub warning: Option<String>,
}

/// Process pending reindex on startup if configured.
///
/// Call this from run() after creating KsmDatabase.
pub fn process_pending_reindex(
    source: &dyn SessionSource,
    db: &KsmDatabase,
) -> Result<PendingReindexResult> {
    if !db.auto_update_enabled() {
        db.clear_pending_reindex()?;
        return Ok(PendingReindexResult {
            session_name: None,
            updated: false,
            warning: None,
        });
    }

    let pending = match db.get_pending_reindex()? {
        Some(id) => id,
        None => {
            return Ok(PendingReindexResult {
                session_name: None,
                updated: false,
                warning: None,
            });
        }
    };

    let (session_id, source_type) = pending;

    let status = match db.get_archive_status_for_source(&session_id, source_type)? {
        Some(ArchiveStatus::Indexed {
            name, archive_id, ..
        }) => (name, archive_id),
        _ => {
            db.clear_pending_reindex()?;
            return Ok(PendingReindexResult {
                session_name: None,
                updated: false,
                warning: None,
            });
        }
    };

    let (name, archive_id) = status;
    let archive = db.get_archive_by_id(archive_id)?;
    let stored_count = archive.message_count;

    let conversation = match source.get_conversation(&session_id, source_type) {
        Ok(c) => c,
        Err(e) => {
            db.clear_pending_reindex()?;
            return Ok(PendingReindexResult {
                session_name: Some(name),
                updated: false,
                warning: Some(e.to_string()),
            });
        }
    };

    let current_count = conversation.history.len() as u32;

    if current_count < stored_count {
        db.clear_pending_reindex()?;
        return Ok(PendingReindexResult {
            session_name: Some(name),
            updated: false,
            warning: None,
        });
    }

    if current_count == stored_count {
        db.clear_pending_reindex()?;
        return Ok(PendingReindexResult {
            session_name: Some(name),
            updated: false,
            warning: None,
        });
    }

    let chunks = extract_chunks_from_conversation(&conversation);
    if let Err(e) = db.update_archive(archive_id, current_count, &chunks) {
        db.clear_pending_reindex()?;
        return Ok(PendingReindexResult {
            session_name: Some(name),
            updated: false,
            warning: Some(e.to_string()),
        });
    }

    db.clear_pending_reindex()?;
    Ok(PendingReindexResult {
        session_name: Some(name),
        updated: true,
        warning: None,
    })
}

// --- Extraction and chunking (private) ---

/// Extract conversation into clean content chunks.
///
/// One chunk per user-assistant exchange. An exchange is everything between
/// one user prompt and the next user prompt.
fn extract_chunks(conversation: &ConversationData) -> Vec<NewChunk> {
    let mut chunks = Vec::new();
    let mut exchange_index: i32 = 0;
    let mut current_user_content = String::new();
    let mut assistant_parts: Vec<String> = Vec::new();
    let mut tool_summaries: Vec<String> = Vec::new();
    let mut has_content = false;

    for entry in &conversation.history {
        // Process user side
        if let Some(user_msg) = &entry.user {
            match &user_msg.content {
                Some(UserContent::Prompt(p)) => {
                    // New exchange -- flush previous if we have content
                    if has_content {
                        let user = clean_pruning_markers(&current_user_content);
                        let assistant = clean_pruning_markers(&assistant_parts.join("\n\n"));
                        let tool_summary = if tool_summaries.is_empty() {
                            None
                        } else {
                            let cleaned = clean_pruning_markers(&tool_summaries.join("\n"));
                            if cleaned.is_empty() {
                                None
                            } else {
                                Some(cleaned)
                            }
                        };

                        if !user.is_empty() || !assistant.is_empty() {
                            chunks.push(NewChunk {
                                exchange_index,
                                user_content: user,
                                assistant_content: assistant,
                                tool_summary,
                            });
                            exchange_index += 1;
                        }
                    }

                    current_user_content = p.prompt.clone();
                    assistant_parts.clear();
                    tool_summaries.clear();
                    has_content = true;
                }
                Some(UserContent::CancelledToolUses(c)) => {
                    if let Some(prompt) = &c.prompt
                        && !prompt.is_empty()
                    {
                        // User interrupted with a message -- treat as new exchange
                        if has_content {
                            let user = clean_pruning_markers(&current_user_content);
                            let assistant = clean_pruning_markers(&assistant_parts.join("\n\n"));
                            let tool_summary = if tool_summaries.is_empty() {
                                None
                            } else {
                                let cleaned = clean_pruning_markers(&tool_summaries.join("\n"));
                                if cleaned.is_empty() {
                                    None
                                } else {
                                    Some(cleaned)
                                }
                            };

                            if !user.is_empty() || !assistant.is_empty() {
                                chunks.push(NewChunk {
                                    exchange_index,
                                    user_content: user,
                                    assistant_content: assistant,
                                    tool_summary,
                                });
                                exchange_index += 1;
                            }
                        }

                        current_user_content = prompt.clone();
                        assistant_parts.clear();
                        tool_summaries.clear();
                        has_content = true;
                    }
                }
                Some(UserContent::ToolUseResults(_)) | None => {
                    // Intermediate entry within same exchange -- skip user side
                }
            }
        }

        // Process assistant side
        if let Some(assistant) = &entry.assistant {
            match assistant {
                AssistantContent::Response(r) => {
                    if !r.content.is_empty() {
                        assistant_parts.push(r.content.clone());
                    }
                }
                AssistantContent::ToolUse(t) => {
                    if !t.content.is_empty() {
                        assistant_parts.push(t.content.clone());
                    }
                    for tool_call in &t.tool_uses {
                        tool_summaries.push(summarise_tool_call(tool_call));
                    }
                }
            }
        }
    }

    // Flush final exchange
    if has_content {
        let user = clean_pruning_markers(&current_user_content);
        let assistant = clean_pruning_markers(&assistant_parts.join("\n\n"));
        let tool_summary = if tool_summaries.is_empty() {
            None
        } else {
            let cleaned = clean_pruning_markers(&tool_summaries.join("\n"));
            if cleaned.is_empty() {
                None
            } else {
                Some(cleaned)
            }
        };

        if !user.is_empty() || !assistant.is_empty() {
            chunks.push(NewChunk {
                exchange_index,
                user_content: user,
                assistant_content: assistant,
                tool_summary,
            });
        }
    }

    chunks
}

/// Detect if a conversation contains pruning markers.
fn is_pruned(conversation: &ConversationData) -> bool {
    for entry in &conversation.history {
        if let Some(user_msg) = &entry.user
            && let Some(UserContent::ToolUseResults(results)) = &user_msg.content
        {
            for result in &results.tool_use_results {
                for content in &result.content {
                    if let ToolResultContent::Text(text) = content
                        && (text == "[Pruned]" || text == "[pruned]")
                    {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// Remove pruning markers from text before indexing.
fn clean_pruning_markers(text: &str) -> String {
    text.replace("[Pruned]", "")
        .replace("[pruned]", "")
        .trim()
        .to_string()
}

// --- Tool call summarisation (private) ---

/// Summarise a single tool call for archive storage.
///
/// Preserves the "what" (tool name, file path, command) without the bulk
/// (file contents, search results, directory listings).
fn summarise_tool_call(tool_call: &ToolCall) -> String {
    let args = &tool_call.args;
    let name = tool_call.name.as_str();

    match name {
        "fs_write" => {
            let command = args["command"].as_str().unwrap_or("unknown");
            let path = args["path"].as_str().unwrap_or("unknown");
            if command == "create" {
                format!("Created {}", path)
            } else {
                format!("Modified {} ({})", path, command)
            }
        }
        "fs_read" => {
            if let Some(operations) = args["operations"].as_array() {
                let summaries: Vec<String> = operations
                    .iter()
                    .map(|op| {
                        let path = op["path"].as_str().unwrap_or("unknown");
                        match op["mode"].as_str().unwrap_or("") {
                            "Search" => {
                                let pattern = op["pattern"].as_str().unwrap_or("?");
                                format!("Searched {} for '{}'", path, pattern)
                            }
                            "Directory" => format!("Listed {}", path),
                            _ => format!("Read {}", path),
                        }
                    })
                    .collect();
                summaries.join("; ")
            } else {
                format!("Used tool: {}", name)
            }
        }
        "execute_bash" => {
            let command = args["command"].as_str().unwrap_or("unknown");
            let truncated: String = command.chars().take(200).collect();
            if truncated.len() < command.len() {
                format!("Ran: {}...", truncated)
            } else {
                format!("Ran: {}", command)
            }
        }
        "code" => {
            let operation = args["operation"].as_str().unwrap_or("unknown");
            match operation {
                "search_symbols" => {
                    let symbol = args["symbol_name"].as_str().unwrap_or("?");
                    format!("Searched symbols: {}", symbol)
                }
                "goto_definition" => {
                    let file = args["file_path"].as_str().unwrap_or("?");
                    let row = args["row"].as_u64().unwrap_or(0);
                    format!("Go to definition: {}:{}", file, row)
                }
                "find_references" => {
                    let file = args["file_path"].as_str().unwrap_or("?");
                    let row = args["row"].as_u64().unwrap_or(0);
                    format!("Find references: {}:{}", file, row)
                }
                "get_document_symbols" => {
                    let file = args["file_path"].as_str().unwrap_or("?");
                    format!("Document symbols: {}", file)
                }
                _ => format!("Code: {}", operation),
            }
        }
        "grep" => {
            let pattern = args["pattern"].as_str().unwrap_or("?");
            let path = args["path"].as_str().unwrap_or(".");
            format!("Searched for \"{}\" in {}", pattern, path)
        }
        "glob" => {
            let pattern = args["pattern"].as_str().unwrap_or("?");
            format!("Glob: {}", pattern)
        }
        "web_search" => {
            let query = args["query"].as_str().unwrap_or("?");
            format!("Web search: {}", query)
        }
        "web_fetch" => {
            let url = args["url"].as_str().unwrap_or("?");
            format!("Fetched: {}", url)
        }
        "thinking" => {
            let thought = args["thought"].as_str().unwrap_or("");
            let truncated: String = thought.chars().take(200).collect();
            format!("Thinking: {}", truncated)
        }
        "todo_list" => {
            let command = args["command"].as_str().unwrap_or("?");
            format!("Todo: {}", command)
        }
        "use_subagent" => {
            let command = args["command"].as_str().unwrap_or("?");
            format!("Subagent: {}", command)
        }
        "knowledge" => {
            let command = args["command"].as_str().unwrap_or("?");
            format!("Knowledge: {}", command)
        }
        _ => format!("Used tool: {}", name),
    }
}
