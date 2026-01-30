use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use regex::Regex;
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
    Delete { indices: String },
}

#[derive(Debug)]
struct Session {
    id: String,
    time_ago: String,
    preview: String,
    msg_count: String,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::List => list_sessions()?,
        Commands::Delete { indices } => delete_sessions(&indices)?,
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

    if sessions.is_empty() {
        println!("No sessions found.");
        return Ok(());
    }

    println!("\nKiro Chat Sessions:\n");
    for (idx, session) in sessions.iter().enumerate() {
        println!("[{}] {} | {} | {}", idx, session.time_ago, session.msg_count, session.preview);
    }
    println!("\nUse 'ksm delete <indices>' to delete sessions (e.g., 'ksm delete 0,2,4')");

    Ok(())
}

fn delete_sessions(indices: &str) -> Result<()> {
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

    println!("\nDeleting {} session(s)...", indices.len());
    
    for &idx in &indices {
        let session = &sessions[idx];
        println!("  [{}] {}", idx, session.preview);
        
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
