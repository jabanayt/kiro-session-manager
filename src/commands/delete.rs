use anyhow::{Context, Result};
use std::io::{self, Write};
use std::process::Command;

use crate::kiro::get_sessions;
use crate::commands::list::{display_sessions_with_metadata, format_session_display};
use crate::models::Session;
use crate::storage::load_metadata;

pub fn delete_sessions(indices: Option<Vec<usize>>, skip_confirm: bool) -> Result<()> {
    let sessions = get_sessions()?;
    
    let indices = match indices {
        Some(idx) => idx,
        None => interactive_delete(&sessions)?,
    };

    delete_by_indices(&sessions, &indices, skip_confirm)
}

fn interactive_delete(sessions: &[Session]) -> Result<Vec<usize>> {
    let metadata = load_metadata()?;
    
    println!();
    display_sessions_with_metadata(sessions, &metadata);
    println!();
    
    print!("Enter sessions to delete (comma-separated): ");
    io::stdout().flush()?;
    
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    
    let indices: Vec<usize> = input
        .trim()
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|s| !s.is_empty())
        .map(|s| s.parse::<usize>())
        .collect::<Result<Vec<_>, _>>()
        .context("Invalid index format")?;
    
    if indices.is_empty() {
        anyhow::bail!("No sessions selected");
    }
    
    Ok(indices)
}

fn delete_by_indices(sessions: &[Session], indices: &[usize], skip_confirm: bool) -> Result<()> {
    for &idx in indices {
        if idx >= sessions.len() {
            anyhow::bail!("Index {} out of range (max: {})", idx, sessions.len() - 1);
        }
    }

    let metadata = load_metadata()?;

    println!("\nSessions to delete:");
    for &idx in indices {
        let session = &sessions[idx];
        let display = format_session_display(session, &metadata, &sessions, true, true);
        println!("  [{}] {} | {}", idx, session.time_ago, display);
    }

    if !skip_confirm {
        print!("\nDelete these {} session(s)? (y/n): ", indices.len());
        io::stdout().flush()?;
        
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        
        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Cancelled.");
            return Ok(());
        }
    }

    println!("\nDeleting {} session(s)...", indices.len());
    
    for &idx in indices {
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
