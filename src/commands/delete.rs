use anyhow::{Context, Result};
use std::io::{self, Write};
use std::process::Command;

use crate::commands::list::{display_filtered_sessions, format_session_display};
use crate::kiro::get_sessions;
use crate::models::Session;
use crate::storage::{
    get_full_chain, get_ordered_chain, load_metadata, relink_around_session, save_metadata,
};

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
    display_filtered_sessions(sessions, &metadata, false);
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

    let mut metadata = load_metadata()?;

    // For single session deletion, check for chain
    if indices.len() == 1 {
        let idx = indices[0];
        let session = &sessions[idx];
        let chain = get_full_chain(&session.id, &metadata, sessions);

        if chain.len() > 1 {
            // Session is part of a chain
            let ordered = get_ordered_chain(&session.id, &metadata, sessions);

            print!("\nSession [{}] is part of a chain: ", idx);

            for (i, chain_id) in ordered.iter().enumerate() {
                if let Some(chain_idx) = sessions.iter().position(|s| &s.id == chain_id) {
                    if i > 0 {
                        print!(" → ");
                    }
                    print!("[{}]", chain_idx);
                }
            }
            println!("\n");

            println!("\nDelete options:");
            println!("  1. Only [{}] (will relink around it)", idx);
            println!("  2. [{}] and all parents", idx);
            println!("  3. Entire chain");
            print!("Choice (1-3) [1]: ");
            io::stdout().flush()?;

            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            let choice = input.trim();
            let choice = if choice.is_empty() { "1" } else { choice };

            match choice {
                "1" => {
                    // Delete only this session, relink around it
                    relink_around_session(&session.id, &mut metadata)?;
                    save_metadata(&metadata)?;
                    delete_session_by_id(&session.id)?;
                    println!("\n✓ Deleted [{}] and relinked chain", idx);
                    return Ok(());
                }
                "2" => {
                    // Delete this session and all parents
                    let mut to_delete = vec![session.id.clone()];
                    let mut current = session.id.clone();
                    while let Some(meta) = metadata.get(&current) {
                        if let Some(parent_id) = &meta.parent_session_id {
                            to_delete.push(parent_id.clone());
                            current = parent_id.clone();
                        } else {
                            break;
                        }
                    }

                    for id in &to_delete {
                        delete_session_by_id(id)?;
                        metadata.remove(id);
                    }
                    save_metadata(&metadata)?;
                    println!("\n✓ Deleted {} session(s)", to_delete.len());
                    return Ok(());
                }
                "3" => {
                    // Delete entire chain
                    for chain_id in &chain {
                        delete_session_by_id(chain_id)?;
                        metadata.remove(chain_id);
                    }
                    save_metadata(&metadata)?;
                    println!("\n✓ Deleted entire chain ({} sessions)", chain.len());
                    return Ok(());
                }
                _ => {
                    anyhow::bail!("Invalid choice");
                }
            }
        }
    }

    // Standard deletion (no chain or multiple sessions)
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
        delete_session_by_id(&session.id)?;
    }

    println!("\nDone!");
    Ok(())
}

fn delete_session_by_id(session_id: &str) -> Result<()> {
    let output = Command::new("kiro-cli")
        .args(["chat", "--delete-session", session_id])
        .output()
        .context("Failed to delete session")?;

    if !output.status.success() {
        anyhow::bail!(
            "Failed to delete session {}: {}",
            session_id,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    Ok(())
}
