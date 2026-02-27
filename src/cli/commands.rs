use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::io::{self, Write};

use crate::cli::display;
use crate::data::{HybridSource, JsonMetadataStore, MetadataStore, SessionSource};
use crate::services::{chains, delete, metadata, resume, sessions};

// --- Clap definitions (from current main.rs lines 15-87) ---

#[derive(Parser)]
#[command(name = "ksm")]
#[command(about = "Kiro Session Manager - manage kiro-cli chat sessions")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// List all chat sessions with numbered indices
    List {
        #[arg(long)]
        show_parents: bool,
        #[arg(long, hide = true)]
        compare_methods: bool,
    },
    /// Delete sessions by index numbers (e.g., "1,3,5" or "1 3 5")
    #[command(alias = "d")]
    Delete {
        indices: Option<Vec<usize>>,
        #[arg(short, long)]
        yes: bool,
    },
    /// Set a custom name for a session
    Name {
        index: usize,
        name: String,
        #[arg(long)]
        chain: bool,
    },
    /// Add tags to a session
    Tag {
        index: usize,
        tags: Vec<String>,
        #[arg(long)]
        chain: bool,
    },
    /// Remove tags from a session
    Untag {
        index: usize,
        tags: Vec<String>,
        #[arg(long)]
        chain: bool,
    },
    /// Clean up metadata for deleted sessions
    CleanMetadata,
    /// Resume a chat session
    #[command(alias = "r")]
    Resume {
        index: Option<usize>,
        #[arg(short, long)]
        last: bool,
        #[arg(short, long)]
        tag: Option<String>,
        #[arg(short, long)]
        name: Option<String>,
    },
    /// Link a child session to a parent session
    Link {
        child_index: usize,
        parent_index: usize,
    },
    /// Unlink a child session from its parent
    Unlink {
        index: usize,
        #[arg(short, long)]
        keep: bool,
    },
    /// Auto-detect and link compacted sessions to their parents
    DetectLinks {
        #[arg(short, long)]
        force: bool,
    },
}

/// Main CLI dispatch. Called from main.rs.
pub fn run(cli: Cli) -> Result<()> {
    let source = HybridSource::new();
    let store = JsonMetadataStore::from_config()
        .context("Failed to initialise metadata store")?;

    match cli.command {
        Commands::List { show_parents, compare_methods } => {
            if compare_methods {
                cmd_compare_methods()?;
            } else {
                cmd_list(&source, &store, show_parents)?;
            }
        }
        Commands::Delete { indices, yes } => {
            cmd_delete(&source, &store, indices, yes)?;
        }
        Commands::Name { index, name, chain } => {
            cmd_name(&source, &store, index, &name, chain)?;
        }
        Commands::Tag { index, tags, chain } => {
            cmd_tag(&source, &store, index, &tags, chain)?;
        }
        Commands::Untag { index, tags, chain } => {
            cmd_untag(&source, &store, index, &tags, chain)?;
        }
        Commands::CleanMetadata => {
            cmd_clean_metadata(&source, &store)?;
        }
        Commands::Resume { index, last, tag, name } => {
            cmd_resume(&source, &store, index, last, tag, name)?;
        }
        Commands::Link { child_index, parent_index } => {
            cmd_link(&source, &store, child_index, parent_index)?;
        }
        Commands::Unlink { index, keep } => {
            cmd_unlink(&source, &store, index, keep)?;
        }
        Commands::DetectLinks { force } => {
            cmd_detect_links(&source, &store, force)?;
        }
    }

    Ok(())
}

// --- Command handlers (each calls services, handles I/O) ---

/// List sessions.
fn cmd_list(
    source: &dyn SessionSource,
    store: &dyn MetadataStore,
    show_parents: bool,
) -> Result<()> {
    let result = sessions::list_sessions(source, store)?;

    if result.all_sessions.is_empty() {
        println!("No sessions found.");
        return Ok(());
    }

    if result.auto_linked > 0 {
        println!(
            "✓ Auto-linked {} compacted session(s) to their parents\n",
            result.auto_linked
        );
    }

    let visible = sessions::visible_session_indices(&result.all_sessions, &result.metadata);
    display::print_session_list(&result.all_sessions, &result.metadata, &visible, show_parents);
    println!("\nUse 'ksm delete <indices>' to delete sessions (e.g., 'ksm delete 0,2,4')");

    Ok(())
}

/// Delete sessions with interactive chain handling.
fn cmd_delete(
    source: &dyn SessionSource,
    store: &dyn MetadataStore,
    indices: Option<Vec<usize>>,
    skip_confirm: bool,
) -> Result<()> {
    let list_result = sessions::list_sessions(source, store)?;
    let all_sessions = &list_result.all_sessions;
    let mut meta = list_result.metadata;

    let indices = match indices {
        Some(idx) => idx,
        None => {
            // Interactive delete: show sessions, prompt for indices
            println!();
            let visible = sessions::visible_session_indices(all_sessions, &meta);
            display::print_session_list(all_sessions, &meta, &visible, false);
            println!();
            print!("Enter sessions to delete (comma-separated): ");
            io::stdout().flush()?;
            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            input
                .trim()
                .split(|c: char| c == ',' || c.is_whitespace())
                .filter(|s| !s.is_empty())
                .map(|s| s.parse::<usize>())
                .collect::<std::result::Result<Vec<_>, _>>()
                .context("Invalid index format")?
        }
    };

    sessions::validate_indices(&indices, all_sessions.len())?;

    // Single session chain check
    if indices.len() == 1 {
        let idx = indices[0];
        let session = &all_sessions[idx];

        if let Some(chain_ctx) = metadata::get_chain_context(&session.id, &meta, all_sessions) {
            // Show chain
            print!("\nSession [{}] is part of a chain: ", idx);
            for (i, chain_id) in chain_ctx.ordered_ids.iter().enumerate() {
                if let Some(chain_idx) = all_sessions.iter().position(|s| &s.id == chain_id) {
                    if i > 0 { print!(" → "); }
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

            let chain_choice = match choice {
                "1" => delete::ChainDeleteChoice::SingleRelink,
                "2" => delete::ChainDeleteChoice::WithParents,
                "3" => delete::ChainDeleteChoice::EntireChain,
                _ => anyhow::bail!("Invalid choice"),
            };

            let result = delete::delete_from_chain(
                &session.id, chain_choice, all_sessions, &mut meta, source, store,
            )?;

            match choice {
                "1" => println!("\n✓ Deleted [{}] and relinked chain", idx),
                "2" => println!("\n✓ Deleted {} session(s)", result.deleted_ids.len()),
                "3" => println!("\n✓ Deleted entire chain ({} sessions)", result.deleted_ids.len()),
                _ => {}
            }
            return Ok(());
        }
    }

    // Standard deletion (no chain or multiple sessions)
    println!("\nSessions to delete:");
    for &idx in &indices {
        let session = &all_sessions[idx];
        let disp = display::format_session_display(session, &meta, all_sessions, true, true);
        let time_ago = display::format_time_ago(session.updated_at);
        println!("  [{}] {} | {}", idx, time_ago, disp);
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

    let ids: Vec<String> = indices.iter().map(|&i| all_sessions[i].id.clone()).collect();
    println!("\nDeleting {} session(s)...", ids.len());
    delete::delete_sessions(&ids, source, &mut meta, store)?;
    println!("\nDone!");

    Ok(())
}

/// Set name with optional chain expansion.
fn cmd_name(
    source: &dyn SessionSource,
    store: &dyn MetadataStore,
    index: usize,
    name: &str,
    apply_to_chain: bool,
) -> Result<()> {
    let list_result = sessions::list_sessions(source, store)?;
    let all_sessions = &list_result.all_sessions;
    let mut meta = list_result.metadata;

    sessions::validate_index(index, all_sessions.len())?;
    let session_id = all_sessions[index].id.clone();

    let scope = if apply_to_chain {
        metadata::MetadataScope::Chain(session_id)
    } else {
        metadata::MetadataScope::Single(session_id)
    };

    let result = metadata::set_name(scope, name, all_sessions, &mut meta, store)?;

    if result.affected_ids.len() > 1 {
        println!("\n✓ Set name for {} sessions: {}", result.affected_ids.len(), name);
    } else {
        println!("Set name for session [{}]: {}", index, name);
    }

    Ok(())
}

/// Add tags with optional chain expansion.
fn cmd_tag(
    source: &dyn SessionSource,
    store: &dyn MetadataStore,
    index: usize,
    tags: &[String],
    apply_to_chain: bool,
) -> Result<()> {
    let list_result = sessions::list_sessions(source, store)?;
    let all_sessions = &list_result.all_sessions;
    let mut meta = list_result.metadata;

    sessions::validate_index(index, all_sessions.len())?;
    let session_id = all_sessions[index].id.clone();

    let scope = if apply_to_chain {
        metadata::MetadataScope::Chain(session_id)
    } else {
        metadata::MetadataScope::Single(session_id)
    };

    let result = metadata::add_tags(scope, tags, all_sessions, &mut meta, store)?;

    if result.affected_ids.len() > 1 {
        println!("\n✓ Added tags to {} sessions: {}", result.affected_ids.len(), tags.join(", "));
    } else {
        println!("Added tags to session [{}]: {}", index, tags.join(", "));
    }

    Ok(())
}

/// Remove tags with optional chain expansion.
fn cmd_untag(
    source: &dyn SessionSource,
    store: &dyn MetadataStore,
    index: usize,
    tags: &[String],
    apply_to_chain: bool,
) -> Result<()> {
    let list_result = sessions::list_sessions(source, store)?;
    let all_sessions = &list_result.all_sessions;
    let mut meta = list_result.metadata;

    sessions::validate_index(index, all_sessions.len())?;
    let session_id = all_sessions[index].id.clone();

    let scope = if apply_to_chain {
        metadata::MetadataScope::Chain(session_id)
    } else {
        metadata::MetadataScope::Single(session_id)
    };

    let result = metadata::remove_tags(scope, tags, all_sessions, &mut meta, store)?;

    if result.affected_ids.len() > 1 {
        println!("\n✓ Removed tags from {} sessions: {}", result.affected_ids.len(), tags.join(", "));
    } else {
        println!("Removed tags from session [{}]: {}", index, tags.join(", "));
    }

    Ok(())
}

/// Clean metadata command.
fn cmd_clean_metadata(
    source: &dyn SessionSource,
    store: &dyn MetadataStore,
) -> Result<()> {
    let all_sessions = source.list_sessions().context("Failed to list sessions")?;
    let mut meta = store.load().context("Failed to load metadata")?;

    let stale = metadata::clean_metadata(&all_sessions, &mut meta, store)?;

    if stale.is_empty() {
        println!("No stale metadata found.");
    } else {
        println!("Removing metadata for {} deleted session(s):", stale.len());
        for (_, display_name) in &stale {
            println!("  - {}", display_name);
        }
        println!("\nDone!");
    }

    Ok(())
}

/// Resume a session.
fn cmd_resume(
    source: &dyn SessionSource,
    store: &dyn MetadataStore,
    index: Option<usize>,
    last: bool,
    tag: Option<String>,
    name: Option<String>,
) -> Result<()> {
    let target = if last {
        resume::ResumeTarget::Last
    } else if let Some(tag) = tag {
        resume::ResumeTarget::Tag(tag)
    } else if let Some(name) = name {
        resume::ResumeTarget::Name(name)
    } else if let Some(idx) = index {
        resume::ResumeTarget::Index(idx)
    } else {
        // Interactive picker
        let list_result = sessions::list_sessions(source, store)?;
        if list_result.all_sessions.is_empty() {
            println!("No sessions found.");
            return Ok(());
        }

        let visible = sessions::visible_session_indices(
            &list_result.all_sessions, &list_result.metadata,
        );
        display::print_session_list(
            &list_result.all_sessions, &list_result.metadata, &visible, false,
        );

        print!("\nSelect session (0-{}): ", list_result.all_sessions.len() - 1);
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let idx: usize = input.trim().parse().context("Invalid number")?;

        resume::ResumeTarget::Index(idx)
    };

    let result = resume::resume(target, source, store)?;

    match result {
        resume::ResumeResult::LaunchDirect => {
            launch_kiro_resume()?;
        }
        resume::ResumeResult::Ready { display_name, .. } => {
            println!("Resuming '{}'...", display_name);
            launch_kiro_resume()?;
        }
        resume::ResumeResult::MultipleMatches { tag, matches } => {
            println!("\nSessions with tag '{}':\n", tag);
            for (i, m) in matches.iter().enumerate() {
                println!("[{}] {}", i, m.display_name);
            }

            print!("\nSelect session (0-{}): ", matches.len() - 1);
            io::stdout().flush()?;
            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            let selection: usize = input.trim().parse().context("Invalid number")?;

            if selection >= matches.len() {
                anyhow::bail!("Index {} out of range (max: {})", selection, matches.len() - 1);
            }

            let retry_target = resume::ResumeTarget::Index(matches[selection].original_index);
            let retry_result = resume::resume(retry_target, source, store)?;
            if let resume::ResumeResult::Ready { display_name, .. } = retry_result {
                println!("Resuming '{}'...", display_name);
            }
            launch_kiro_resume()?;
        }
    }

    Ok(())
}

/// Link two sessions.
fn cmd_link(
    source: &dyn SessionSource,
    store: &dyn MetadataStore,
    child_index: usize,
    parent_index: usize,
) -> Result<()> {
    let list_result = sessions::list_sessions(source, store)?;
    let all_sessions = &list_result.all_sessions;
    let mut meta = list_result.metadata;

    sessions::validate_index(child_index, all_sessions.len())?;
    sessions::validate_index(parent_index, all_sessions.len())?;

    let child_id = all_sessions[child_index].id.clone();
    let parent_id = all_sessions[parent_index].id.clone();

    let result = match chains::link_sessions(&child_id, &parent_id, false, &mut meta, store) {
        Ok(result) => result,
        Err(crate::error::KsmError::MetadataConflict { .. }) => {
            println!("⚠ Warning: Session [{}] already has metadata:", child_index);
            if let Some(existing) = meta.get(&child_id) {
                if let Some(name) = &existing.name {
                    println!("  Name: \"{}\"", name);
                }
                if !existing.tags.is_empty() {
                    println!(
                        "  Tags: {}",
                        existing.tags.iter().cloned().collect::<Vec<_>>().join(", ")
                    );
                }
            }
            println!("\nParent [{}] has:", parent_index);
            if let Some(parent_meta) = meta.get(&parent_id) {
                if let Some(name) = &parent_meta.name {
                    println!("  Name: \"{}\"", name);
                }
                if !parent_meta.tags.is_empty() {
                    println!(
                        "  Tags: {}",
                        parent_meta.tags.iter().cloned().collect::<Vec<_>>().join(", ")
                    );
                }
            } else {
                println!("  (no metadata)");
            }

            print!("\nThis will REPLACE child's metadata with parent's.\nContinue? (y/n): ");
            io::stdout().flush()?;
            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            if !input.trim().eq_ignore_ascii_case("y") {
                println!("Cancelled.");
                return Ok(());
            }

            chains::link_sessions(&child_id, &parent_id, true, &mut meta, store)?
        }
        Err(e) => return Err(e.into()),
    };

    println!("✓ Linked session [{}] to parent [{}]", child_index, parent_index);
    if let Some(name) = &result.inherited_name {
        println!("✓ Inherited name: \"{}\"", name);
    }
    if !result.inherited_tags.is_empty() {
        println!(
            "✓ Inherited tags: {}",
            result.inherited_tags.iter().cloned().collect::<Vec<_>>().join(", ")
        );
    }

    Ok(())
}

/// Unlink a session.
fn cmd_unlink(
    source: &dyn SessionSource,
    store: &dyn MetadataStore,
    index: usize,
    keep_metadata: bool,
) -> Result<()> {
    let list_result = sessions::list_sessions(source, store)?;
    let all_sessions = &list_result.all_sessions;
    let mut meta = list_result.metadata;

    sessions::validate_index(index, all_sessions.len())?;
    let session_id = &all_sessions[index].id;

    let clear = if !keep_metadata {
        print!("Keep inherited name and tags? (y/n): ");
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        !input.trim().eq_ignore_ascii_case("y")
    } else {
        false
    };

    let result = chains::execute_unlink(session_id, clear, &mut meta, store)?;

    let parent_idx = all_sessions
        .iter()
        .position(|s| s.id == result.former_parent_id);

    if let Some(pidx) = parent_idx {
        println!("✓ Unlinked session [{}] from parent [{}]", index, pidx);
    } else {
        println!("✓ Unlinked session [{}]", index);
    }

    Ok(())
}

/// Detect and interactively link continuations.
fn cmd_detect_links(
    source: &dyn SessionSource,
    store: &dyn MetadataStore,
    force: bool,
) -> Result<()> {
    let list_result = sessions::list_sessions(source, store)?;
    let all_sessions = &list_result.all_sessions;
    let mut meta = list_result.metadata;

    if all_sessions.is_empty() {
        println!("No sessions found.");
        return Ok(());
    }

    println!("Scanning for compacted sessions...\n");

    let candidates = chains::detect_unlinked_continuations(all_sessions, &meta, source, force)?;

    if candidates.is_empty() {
        println!("No unlinked compacted sessions found.");
        return Ok(());
    }

    for candidate in candidates {
        let child_idx = all_sessions.iter().position(|s| s.id == candidate.child.id).unwrap();
        let parent_idx = all_sessions.iter().position(|s| s.id == candidate.parent_id).unwrap();
        let parent = &all_sessions[parent_idx];

        let child_display = display::format_session_display(
            &candidate.child, &meta, all_sessions, false, false,
        );
        let parent_display = display::format_session_display(
            parent, &meta, all_sessions, false, false,
        );
        let child_time = display::format_time_ago(candidate.child.updated_at);
        let parent_time = display::format_time_ago(parent.updated_at);

        println!("[{}] {} ({})", child_idx, child_display, child_time);
        println!("    might continue from");
        println!("[{}] {} ({})", parent_idx, parent_display, parent_time);

        print!("\nLink them? (y/n): ");
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        if input.trim().eq_ignore_ascii_case("y") {
            chains::link_sessions(&candidate.child.id, &candidate.parent_id, true, &mut meta, store)?;
            println!("✓ Linked [{}] to [{}]\n", child_idx, parent_idx);
        } else {
            println!("Skipped.\n");
        }
    }

    Ok(())
}

/// Compare database and CLI methods (testing only).
fn cmd_compare_methods() -> Result<()> {
    use crate::data::{DatabaseSource, KiroCliSource};

    eprintln!("=== Comparing Database vs CLI Methods ===\n");

    let db_source = DatabaseSource::new();
    let cli_source = KiroCliSource::new();

    let result = match sessions::compare_sources(&db_source, &cli_source) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Comparison failed: {}", e);
            return Ok(());
        }
    };

    eprintln!("Database: {} sessions", result.source_a_count);
    eprintln!("CLI: {} sessions\n", result.source_b_count);

    if result.source_a_count != result.source_b_count {
        eprintln!(
            "Session count mismatch: DB={}, CLI={}\n",
            result.source_a_count, result.source_b_count
        );
    }

    if result.differences.is_empty() {
        eprintln!("All sessions match! Database method is accurate.");
    } else {
        let mut current_idx = None;
        for diff in &result.differences {
            if current_idx != Some(diff.index) {
                if current_idx.is_some() { eprintln!(); }
                eprintln!("Session [{}] differences:", diff.index);
                current_idx = Some(diff.index);
            }
            eprintln!("  {}: DB='{}' vs CLI='{}'", diff.field, diff.source_a, diff.source_b);
        }
        let session_count = result.differences.iter()
            .map(|d| d.index)
            .collect::<std::collections::HashSet<_>>()
            .len();
        eprintln!("\nFound differences in {} session(s)", session_count);
    }

    Ok(())
}

// --- Shared helpers ---

/// Launch kiro-cli in resume mode (takes over the terminal).
fn launch_kiro_resume() -> Result<()> {
    let status = std::process::Command::new("kiro-cli")
        .args(["chat", "--resume"])
        .status()
        .context("Failed to execute kiro-cli")?;

    if !status.success() {
        anyhow::bail!("kiro-cli exited with error");
    }

    Ok(())
}
