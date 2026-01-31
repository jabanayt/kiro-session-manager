use anyhow::{Context, Result};
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;

use crate::commands::list::{display_sessions_with_metadata, format_session_display};
use crate::kiro::get_sessions;
use crate::storage::{cleanup_stale_metadata, load_metadata};

/// Resume a session by index using database timestamp manipulation
fn resume_session_by_db(index: usize) -> Result<()> {
    let sessions = get_sessions()?;
    
    if index >= sessions.len() {
        anyhow::bail!("Index {} out of range (max: {})", index, sessions.len() - 1);
    }
    
    let target_session = &sessions[index];
    let current_dir = std::env::current_dir()?;
    
    // Get the real database path
    let real_db_path = PathBuf::from(std::env::var("HOME")?)
        .join(".local/share/kiro-cli/data.sqlite3");
    
    // Update the target session's timestamp to make it most recent
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis();
    
    let output = Command::new("sqlite3")
        .arg(&real_db_path)
        .arg(format!(
            "UPDATE conversations_v2 SET updated_at = {} WHERE key='{}' AND conversation_id='{}';",
            timestamp,
            current_dir.display(),
            target_session.id
        ))
        .output()
        .context("Failed to execute sqlite3. Is it installed?")?;
    
    if !output.status.success() {
        anyhow::bail!("Failed to update database: {}", String::from_utf8_lossy(&output.stderr));
    }
    
    // Execute kiro-cli normally - it will resume the "most recent" session (which we just made it)
    let status = Command::new("kiro-cli")
        .args(&["chat", "--resume"])
        .status()
        .context("Failed to execute kiro-cli")?;
    
    if !status.success() {
        anyhow::bail!("kiro-cli exited with error");
    }
    
    Ok(())
}

pub fn resume_session(index: usize) -> Result<()> {
    resume_session_by_db(index)
}

pub fn interactive_resume() -> Result<()> {
    let sessions = get_sessions()?;
    let mut metadata = load_metadata()?;
    cleanup_stale_metadata(&mut metadata, &sessions)?;
    
    if sessions.is_empty() {
        println!("No sessions found.");
        return Ok(());
    }
    
    // Display sessions using shared helper
    display_sessions_with_metadata(&sessions, &metadata);
    
    // Prompt for selection
    print!("\nSelect session (0-{}): ", sessions.len() - 1);
    std::io::stdout().flush()?;
    
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    
    let index: usize = input.trim().parse()
        .context("Invalid number")?;
    
    if index >= sessions.len() {
        anyhow::bail!("Index {} out of range (max: {})", index, sessions.len() - 1);
    }
    
    // Resume selected session
    resume_session(index)?;
    
    Ok(())
}

pub fn resume_by_tag(tag: &str) -> Result<()> {
    let sessions = get_sessions()?;
    let metadata = load_metadata()?;
    
    // Find sessions with this tag
    let matches: Vec<(usize, &crate::models::Session)> = sessions.iter()
        .enumerate()
        .filter(|(_, s)| {
            metadata.get(&s.id)
                .map(|m| m.tags.contains(tag))
                .unwrap_or(false)
        })
        .collect();
    
    match matches.len() {
        0 => anyhow::bail!("No sessions found with tag '{}'", tag),
        1 => {
            let (index, _) = matches[0];
            println!("Resuming session with tag '{}'...", tag);
            resume_session(index)?;
        }
        _ => {
            // Multiple matches - show picker
            println!("\nSessions with tag '{}':\n", tag);
            for (idx, (_orig_idx, session)) in matches.iter().enumerate() {
                let display = format_session_display(session, &metadata, &sessions, false);
                println!("[{}] {} | {} | {}", idx, session.time_ago, session.msg_count, display);
            }
            
            print!("\nSelect session (0-{}): ", matches.len() - 1);
            std::io::stdout().flush()?;
            
            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
            let selection: usize = input.trim().parse()
                .context("Invalid number")?;
            
            if selection >= matches.len() {
                anyhow::bail!("Index {} out of range (max: {})", selection, matches.len() - 1);
            }
            
            let (orig_index, _) = matches[selection];
            resume_session(orig_index)?;
        }
    }
    
    Ok(())
}

pub fn resume_by_name(name: &str) -> Result<()> {
    let sessions = get_sessions()?;
    let metadata = load_metadata()?;
    
    // Find session with exact name match
    let found = sessions.iter()
        .enumerate()
        .find(|(_, s)| {
            metadata.get(&s.id)
                .and_then(|m| m.name.as_deref())
                .map(|n| n == name)
                .unwrap_or(false)
        });
    
    match found {
        Some((index, _)) => {
            println!("Resuming session '{}'...", name);
            resume_session(index)?;
        }
        None => anyhow::bail!("No session found with name '{}'", name),
    }
    
    Ok(())
}

pub fn resume_last() -> Result<()> {
    let status = std::process::Command::new("kiro-cli")
        .args(&["chat", "--resume"])
        .status()
        .context("Failed to execute kiro-cli")?;
    
    if !status.success() {
        anyhow::bail!("Failed to resume last session");
    }
    
    Ok(())
}
