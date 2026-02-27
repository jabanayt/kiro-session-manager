use anyhow::{Context, Result};
use std::collections::HashSet;
use std::io::{self, Write};

use crate::kiro::get_sessions;
use crate::storage::{get_full_chain, get_ordered_chain, load_metadata, save_metadata};

pub fn set_name(index: usize, name: &str, apply_to_chain: bool) -> Result<()> {
    let sessions = get_sessions()?;
    let mut metadata = load_metadata()?;

    if index >= sessions.len() {
        anyhow::bail!("Index {} out of range (max: {})", index, sessions.len() - 1);
    }

    let session = &sessions[index];
    let current_dir = std::env::current_dir()?.to_string_lossy().to_string();

    let target_sessions = if apply_to_chain {
        // Check if session is part of a chain
        let chain = get_full_chain(&session.id, &metadata, &sessions);

        if chain.len() > 1 {
            // Show chain and prompt for confirmation
            let ordered = get_ordered_chain(&session.id, &metadata, &sessions);

            print!("\nSession [{}] is part of a chain: ", index);

            for (i, chain_id) in ordered.iter().enumerate() {
                if let Some(chain_idx) = sessions.iter().position(|s| &s.id == chain_id) {
                    if i > 0 {
                        print!(" → ");
                    }
                    print!("[{}]", chain_idx);
                }
            }
            println!();

            print!("\nApply name \"{}\" to entire chain? (y/n): ", name);
            io::stdout().flush()?;

            let mut input = String::new();
            io::stdin().read_line(&mut input)?;

            if input.trim().eq_ignore_ascii_case("y") {
                chain
            } else {
                vec![session.id.clone()]
            }
        } else {
            vec![session.id.clone()]
        }
    } else {
        vec![session.id.clone()]
    };

    // Apply name to selected sessions
    for session_id in &target_sessions {
        let entry = metadata.entry(session_id.clone()).or_default();
        entry.name = Some(name.to_string());
        entry.directory = Some(current_dir.clone());
    }

    save_metadata(&metadata)?;

    if target_sessions.len() > 1 {
        println!(
            "\n✓ Set name for {} sessions: {}",
            target_sessions.len(),
            name
        );
    } else {
        println!("Set name for session [{}]: {}", index, name);
    }

    Ok(())
}

pub fn add_tags(index: usize, tags: &[String]) -> Result<()> {
    let sessions = get_sessions()?;
    let mut metadata = load_metadata()?;

    if index >= sessions.len() {
        anyhow::bail!("Index {} out of range (max: {})", index, sessions.len() - 1);
    }

    let session = &sessions[index];
    let current_dir = std::env::current_dir()?.to_string_lossy().to_string();

    // Check if session is part of a chain
    let chain = get_full_chain(&session.id, &metadata, &sessions);

    let target_sessions = if chain.len() > 1 {
        // Session is part of a chain
        let ordered = get_ordered_chain(&session.id, &metadata, &sessions);

        print!("\nSession [{}] is part of a chain: ", index);

        for (i, chain_id) in ordered.iter().enumerate() {
            if let Some(chain_idx) = sessions.iter().position(|s| &s.id == chain_id) {
                if i > 0 {
                    print!(" → ");
                }
                print!("[{}]", chain_idx);
            }
        }
        println!("\n");

        println!("Apply tags to:");
        println!("  1. Only [{}]", index);
        println!("  2. Entire chain");
        print!("Choice (1-2) [2]: ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let choice = input.trim();
        let choice = if choice.is_empty() { "2" } else { choice };

        match choice {
            "1" => vec![session.id.clone()],
            "2" => chain,
            _ => anyhow::bail!("Invalid choice"),
        }
    } else {
        vec![session.id.clone()]
    };

    // Apply tags to selected sessions
    for session_id in &target_sessions {
        let entry = metadata.entry(session_id.clone()).or_default();
        entry.directory = Some(current_dir.clone());

        for tag in tags {
            entry.tags.insert(tag.clone());
        }
    }

    save_metadata(&metadata)?;

    if target_sessions.len() > 1 {
        println!(
            "\n✓ Added tags to {} sessions: {}",
            target_sessions.len(),
            tags.join(", ")
        );
    } else {
        println!("Added tags to session [{}]: {}", index, tags.join(", "));
    }

    Ok(())
}

pub fn remove_tags(index: usize, tags: &[String]) -> Result<()> {
    let sessions = get_sessions()?;
    let mut metadata = load_metadata()?;

    if index >= sessions.len() {
        anyhow::bail!("Index {} out of range (max: {})", index, sessions.len() - 1);
    }

    let session = &sessions[index];

    // Check if session is part of a chain
    let chain = get_full_chain(&session.id, &metadata, &sessions);

    let target_sessions = if chain.len() > 1 {
        // Session is part of a chain
        let ordered = get_ordered_chain(&session.id, &metadata, &sessions);

        print!("\nSession [{}] is part of a chain: ", index);

        for (i, chain_id) in ordered.iter().enumerate() {
            if let Some(chain_idx) = sessions.iter().position(|s| &s.id == chain_id) {
                if i > 0 {
                    print!(" → ");
                }
                print!("[{}]", chain_idx);
            }
        }
        println!("\n");

        println!("Remove tags from:");
        println!("  1. Only [{}]", index);
        println!("  2. Entire chain");
        print!("Choice (1-2) [2]: ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let choice = input.trim();
        let choice = if choice.is_empty() { "2" } else { choice };

        match choice {
            "1" => vec![session.id.clone()],
            "2" => chain,
            _ => anyhow::bail!("Invalid choice"),
        }
    } else {
        vec![session.id.clone()]
    };

    // Remove tags from selected sessions
    for session_id in &target_sessions {
        if let Some(entry) = metadata.get_mut(session_id) {
            for tag in tags {
                entry.tags.remove(tag);
            }
        }
    }

    save_metadata(&metadata)?;

    if target_sessions.len() > 1 {
        println!(
            "\n✓ Removed tags from {} sessions: {}",
            target_sessions.len(),
            tags.join(", ")
        );
    } else {
        println!("Removed tags from session [{}]: {}", index, tags.join(", "));
    }

    Ok(())
}

pub fn clean_metadata() -> Result<()> {
    let sessions = get_sessions()?;
    let mut metadata = load_metadata()?;

    let current_dir = std::env::current_dir()
        .context("Failed to get current directory")?
        .to_string_lossy()
        .to_string();

    let session_ids: HashSet<_> = sessions.iter().map(|s| s.id.as_str()).collect();
    let stale_ids: Vec<_> = metadata
        .iter()
        .filter(|(id, meta)| {
            // Only consider entries from current directory
            if let Some(dir) = &meta.directory {
                dir == &current_dir && !session_ids.contains(id.as_str())
            } else {
                // Legacy entries without directory - skip (could belong to any directory)
                false
            }
        })
        .map(|(id, _)| id.clone())
        .collect();

    if stale_ids.is_empty() {
        println!("No stale metadata found.");
        return Ok(());
    }

    println!(
        "Removing metadata for {} deleted session(s):",
        stale_ids.len()
    );
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
