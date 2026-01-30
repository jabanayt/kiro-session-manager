use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

#[derive(Parser)]
#[command(name = "ksm")]
#[command(about = "Kiro Session Manager - manage kiro-cli chat sessions")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// List all chat sessions with numbered indices
    List,
    /// Delete sessions by index numbers (e.g., "1,3,5" or "1 3 5")
    Delete {
        indices: String,
        /// Skip confirmation prompt
        #[arg(short, long)]
        yes: bool,
    },
    /// Set a custom name for a session
    Name {
        index: usize,
        name: String,
    },
    /// Add tags to a session
    Tag {
        index: usize,
        tags: Vec<String>,
    },
    /// Remove tags from a session
    Untag {
        index: usize,
        tags: Vec<String>,
    },
    /// Clean up metadata for deleted sessions
    CleanMetadata,
}

#[derive(Debug)]
struct Session {
    id: String,
    time_ago: String,
    preview: String,
    msg_count: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
struct SessionMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "HashSet::is_empty", default)]
    tags: HashSet<String>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::List => list_sessions()?,
        Commands::Delete { indices, yes } => delete_sessions(&indices, yes)?,
        Commands::Name { index, name } => set_name(index, &name)?,
        Commands::Tag { index, tags } => add_tags(index, &tags)?,
        Commands::Untag { index, tags } => remove_tags(index, &tags)?,
        Commands::CleanMetadata => clean_metadata()?,
    }

    Ok(())
}

fn get_sessions() -> Result<Vec<Session>> {
    let output = Command::new("kiro-cli")
        .args(["chat", "--list-sessions"])
        .output()
        .context("Failed to execute kiro-cli")?;

    let stderr = String::from_utf8_lossy(&output.stderr);
    parse_sessions(&stderr)
}

fn parse_sessions(output: &str) -> Result<Vec<Session>> {
    let re = Regex::new(
        r"Chat SessionId: \x1B\[38;5;\d+m([a-f0-9-]+)\n\x1B\[0m\s+\x1B\[2m(.+?)\x1B\[0m \| (.+?) \| \x1B\[2m(\d+ msgs?)\x1B\[0m"
    )?;

    let sessions: Vec<Session> = re
        .captures_iter(output)
        .map(|cap| Session {
            id: cap[1].to_string(),
            time_ago: cap[2].to_string(),
            preview: cap[3].to_string(),
            msg_count: cap[4].to_string(),
        })
        .collect();

    Ok(sessions)
}

fn list_sessions() -> Result<()> {
    let sessions = get_sessions()?;
    let mut metadata = load_metadata()?;
    
    // Silent cleanup of stale metadata
    cleanup_stale_metadata(&mut metadata, &sessions)?;

    if sessions.is_empty() {
        println!("No sessions found.");
        return Ok(());
    }

    println!("\nKiro Chat Sessions:\n");
    for (idx, session) in sessions.iter().enumerate() {
        let meta = metadata.get(&session.id);
        
        // Build display string
        let mut display = String::new();
        
        // Add tags if present
        if let Some(meta) = meta {
            if !meta.tags.is_empty() {
                let mut tags: Vec<_> = meta.tags.iter().collect();
                tags.sort();
                for tag in tags {
                    display.push_str(&format!("[{}] ", tag));
                }
            }
        }
        
        // Add name or preview
        if let Some(meta) = meta {
            if let Some(name) = &meta.name {
                display.push_str(name);
            } else {
                display.push_str(&session.preview);
            }
        } else {
            display.push_str(&session.preview);
        }
        
        println!("[{}] {} | {} | {}", idx, session.time_ago, session.msg_count, display);
    }
    println!("\nUse 'ksm delete <indices>' to delete sessions (e.g., 'ksm delete 0,2,4')");

    Ok(())
}

fn delete_sessions(indices: &str, skip_confirm: bool) -> Result<()> {
    let sessions = get_sessions()?;
    
    let indices: Vec<usize> = indices
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|s| !s.is_empty())
        .map(|s| s.parse::<usize>())
        .collect::<Result<Vec<_>, _>>()
        .context("Invalid index format")?;

    for &idx in &indices {
        if idx >= sessions.len() {
            anyhow::bail!("Index {} out of range (max: {})", idx, sessions.len() - 1);
        }
    }

    println!("\nSessions to delete:");
    for &idx in &indices {
        let session = &sessions[idx];
        println!("  [{}] {} | {}", idx, session.time_ago, session.preview);
    }

    if !skip_confirm {
        print!("\nDelete these {} session(s)? (y/n): ", indices.len());
        std::io::Write::flush(&mut std::io::stdout())?;
        
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        
        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Cancelled.");
            return Ok(());
        }
    }

    println!("\nDeleting {} session(s)...", indices.len());
    
    for &idx in &indices {
        let session = &sessions[idx];
        
        let output = Command::new("kiro-cli")
            .args(["chat", "--delete-session", &session.id])
            .output()
            .context("Failed to delete session")?;

        if !output.status.success() {
            eprintln!("Failed to delete session {}: {}", session.id, String::from_utf8_lossy(&output.stderr));
        }
    }

    println!("\nDone!");
    Ok(())
}

fn metadata_path() -> Result<PathBuf> {
    let home = std::env::var("HOME").context("HOME environment variable not set")?;
    let ksm_dir = PathBuf::from(home).join(".ksm");
    fs::create_dir_all(&ksm_dir)?;
    Ok(ksm_dir.join("metadata.json"))
}

fn load_metadata() -> Result<HashMap<String, SessionMetadata>> {
    let path = metadata_path()?;
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let content = fs::read_to_string(&path)?;
    let metadata = serde_json::from_str(&content)?;
    Ok(metadata)
}

fn save_metadata(metadata: &HashMap<String, SessionMetadata>) -> Result<()> {
    let path = metadata_path()?;
    let content = serde_json::to_string_pretty(metadata)?;
    fs::write(&path, content)?;
    Ok(())
}

fn cleanup_stale_metadata(metadata: &mut HashMap<String, SessionMetadata>, sessions: &[Session]) -> Result<()> {
    let session_ids: HashSet<_> = sessions.iter().map(|s| s.id.as_str()).collect();
    let stale_ids: Vec<_> = metadata.keys()
        .filter(|id| !session_ids.contains(id.as_str()))
        .cloned()
        .collect();
    
    if !stale_ids.is_empty() {
        for id in stale_ids {
            metadata.remove(&id);
        }
        save_metadata(metadata)?;
    }
    
    Ok(())
}

fn set_name(index: usize, name: &str) -> Result<()> {
    let sessions = get_sessions()?;
    let mut metadata = load_metadata()?;
    
    if index >= sessions.len() {
        anyhow::bail!("Index {} out of range (max: {})", index, sessions.len() - 1);
    }
    
    let session = &sessions[index];
    metadata.entry(session.id.clone())
        .or_default()
        .name = Some(name.to_string());
    
    save_metadata(&metadata)?;
    println!("Set name for session [{}]: {}", index, name);
    Ok(())
}

fn add_tags(index: usize, tags: &[String]) -> Result<()> {
    let sessions = get_sessions()?;
    let mut metadata = load_metadata()?;
    
    if index >= sessions.len() {
        anyhow::bail!("Index {} out of range (max: {})", index, sessions.len() - 1);
    }
    
    let session = &sessions[index];
    let entry = metadata.entry(session.id.clone()).or_default();
    
    for tag in tags {
        entry.tags.insert(tag.clone());
    }
    
    save_metadata(&metadata)?;
    println!("Added tags to session [{}]: {}", index, tags.join(", "));
    Ok(())
}

fn remove_tags(index: usize, tags: &[String]) -> Result<()> {
    let sessions = get_sessions()?;
    let mut metadata = load_metadata()?;
    
    if index >= sessions.len() {
        anyhow::bail!("Index {} out of range (max: {})", index, sessions.len() - 1);
    }
    
    let session = &sessions[index];
    if let Some(entry) = metadata.get_mut(&session.id) {
        for tag in tags {
            entry.tags.remove(tag);
        }
        save_metadata(&metadata)?;
        println!("Removed tags from session [{}]: {}", index, tags.join(", "));
    } else {
        println!("No metadata found for session [{}]", index);
    }
    
    Ok(())
}

fn clean_metadata() -> Result<()> {
    let sessions = get_sessions()?;
    let mut metadata = load_metadata()?;
    
    let session_ids: HashSet<_> = sessions.iter().map(|s| s.id.as_str()).collect();
    let stale_ids: Vec<_> = metadata.keys()
        .filter(|id| !session_ids.contains(id.as_str()))
        .cloned()
        .collect();
    
    if stale_ids.is_empty() {
        println!("No stale metadata found.");
        return Ok(());
    }
    
    println!("Removing metadata for {} deleted session(s):", stale_ids.len());
    for id in &stale_ids {
        if let Some(meta) = metadata.get(id) {
            let display = meta.name.as_deref().unwrap_or(&id[..8]);
            println!("  - {}", display);
        }
        metadata.remove(id);
    }
    
    save_metadata(&metadata)?;
    println!("\nDone!");
    Ok(())
}
