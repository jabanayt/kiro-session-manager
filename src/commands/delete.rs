use anyhow::{Context, Result};
use std::process::Command;

use crate::kiro::get_sessions;

pub fn delete_sessions(indices: &str, skip_confirm: bool) -> Result<()> {
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
