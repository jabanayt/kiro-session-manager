//! Archive service: extract, clean, chunk, save, search, delete.
//!
//! This is the core business logic for the archive feature. It reads
//! conversations via SessionSource, processes them into clean chunks,
//! and saves/searches via ArchiveStore.

use crate::data::{ArchiveStore, SessionSource};
use crate::error::{KsmError, Result};
use crate::models::{
    Archive, ArchiveResult, AssistantContent, Chunk, ConversationData, DeleteArchiveResult,
    NewArchive, NewChunk, SearchQuery, SearchResult, ShowArchiveResult, ToolCall,
    ToolResultContent, UserContent,
};

// --- Archive operation ---

/// Archive a session: extract conversation, clean, chunk, and save.
///
/// This is a complete operation. The CLI provides the name and tags
/// (after prompting the user if needed). The service does the rest.
pub fn archive_session(
    session_id: &str,
    name: &str,
    tags: Vec<String>,
    session_created_at: i64,
    directory: &str,
    source: &dyn SessionSource,
    archive_store: &dyn ArchiveStore,
) -> Result<ArchiveResult> {
    if let Some(existing_name) = archive_store.is_archived(session_id)? {
        return Err(KsmError::AlreadyArchived(existing_name));
    }

    let conversation = source.get_conversation(session_id)?;
    let pruned = is_pruned(&conversation);
    let chunks = extract_chunks(&conversation);

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
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
    };

    archive_store.save_archive(&new_archive, &chunks)?;

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
    archive_store: &dyn ArchiveStore,
) -> Result<Vec<SearchResult>> {
    let query = SearchQuery {
        query: query_text.to_string(),
        directory: directory.to_string(),
        limit,
    };
    archive_store.search(&query)
}

/// Get the full exchange content for a specific search result.
///
/// Used by --expand to show the complete exchange instead of just a snippet.
pub fn get_expanded_result(
    archive_name: &str,
    exchange_index: i32,
    directory: &str,
    archive_store: &dyn ArchiveStore,
) -> Result<Chunk> {
    let archive = archive_store.get_archive(archive_name, directory)?;
    let chunks = archive_store.get_chunks(archive.id)?;

    chunks
        .into_iter()
        .find(|c| c.exchange_index == exchange_index)
        .ok_or_else(|| {
            KsmError::InvalidInput(format!(
                "Exchange {} not found in archive '{}'.",
                exchange_index, archive_name
            ))
        })
}

// --- List operation ---

/// List all archives for the given project directory.
pub fn list_archives(directory: &str, archive_store: &dyn ArchiveStore) -> Result<Vec<Archive>> {
    archive_store.list_archives(directory)
}

// --- Show operation ---

/// Get a full archived conversation for browsing.
pub fn show_archive(
    archive_name: &str,
    directory: &str,
    archive_store: &dyn ArchiveStore,
) -> Result<ShowArchiveResult> {
    let archive = archive_store.get_archive(archive_name, directory)?;
    let chunks = archive_store.get_chunks(archive.id)?;
    Ok(ShowArchiveResult { archive, chunks })
}

// --- Delete operation ---

/// Get archive info for confirmation prompts (e.g., before delete).
pub fn get_archive_info(
    name: &str,
    directory: &str,
    archive_store: &dyn ArchiveStore,
) -> Result<Archive> {
    archive_store.get_archive(name, directory)
}

/// Delete an archive and all its indexed content.
pub fn delete_archive(
    archive_name: &str,
    directory: &str,
    archive_store: &dyn ArchiveStore,
) -> Result<DeleteArchiveResult> {
    let archive = archive_store.get_archive(archive_name, directory)?;
    archive_store.delete_archive(archive.id)?;

    Ok(DeleteArchiveResult {
        archive_name: archive_name.to_string(),
        message_count: archive.message_count,
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
