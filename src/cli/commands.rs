use std::io::{self, Write};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use crate::cli::display;
use crate::cli::notices::{Notice, render_notices};
use crate::cli::pager;
use crate::cli::styles;
use crate::data::{HybridSource, KsmDatabase, SessionSource};
use crate::error::KsmError;
use crate::services::sessions::SessionContext;
use crate::services::{archive, chains, delete, metadata, resume, sessions};

// --- Clap definitions ---

#[derive(Parser)]
#[command(name = "ksm")]
#[command(version)]
#[command(about = "Kiro Session Manager - manage kiro-cli chat sessions")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    // === Session Management ===
    /// List all chat sessions with numbered indices
    List {
        #[arg(long)]
        show_parents: bool,
        #[arg(long, hide = true)]
        compare_methods: bool,
    },
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
    /// Delete sessions by index numbers (e.g., "1,3,5" or "1 3 5")
    #[command(alias = "d")]
    Delete {
        #[arg(value_delimiter = ',')]
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
        #[arg(num_args = 1..)]
        tags: Vec<String>,
        #[arg(long)]
        chain: bool,
    },
    /// Remove tags from a session
    Untag {
        index: usize,
        #[arg(num_args = 1..)]
        tags: Vec<String>,
        #[arg(long)]
        chain: bool,
    },
    /// Clean up metadata for deleted sessions
    CleanMetadata,

    // === Indexing & Search ===
    /// Index a session for search (keeps session in Kiro)
    Index {
        /// Session index to index
        index: usize,
        /// Index name (prompted if not provided)
        #[arg(long)]
        name: Option<String>,
        /// Tags (space-separated, prompted if not provided)
        #[arg(long)]
        tags: Option<Vec<String>>,
    },
    /// Remove index from a session (keeps session in Kiro)
    Unindex {
        /// Session index (from ksm list)
        index: usize,
    },
    /// Update search index for indexed sessions
    Reindex {
        /// Session index to reindex (from ksm list), or all if omitted
        index: Option<usize>,
    },
    /// Search archived and indexed sessions
    Search {
        /// FTS5 search query (supports "exact phrase", AND, OR, NOT, prefix*)
        query: String,
        /// Maximum number of results (default: 50)
        #[arg(long, default_value = "50")]
        limit: u32,
        /// Show full exchange for result N
        #[arg(long)]
        expand: Option<usize>,
        /// Disable pager (print directly to stdout)
        #[arg(long)]
        no_pager: bool,
    },

    // === Archives ===
    /// Archive a session for future search
    Archive {
        /// Session index to archive
        index: usize,
        /// Archive name (prompted if not provided)
        #[arg(long)]
        name: Option<String>,
        /// Tags (space-separated, prompted if not provided)
        #[arg(long)]
        tags: Option<Vec<String>>,
    },
    /// List all archives for the current project
    ListArchives,
    /// Browse a full archived conversation
    ShowArchive {
        /// Archive index (from list-archives) or name
        target: String,
        /// Jump to a specific exchange
        #[arg(long)]
        exchange: Option<i32>,
        /// Disable pager (print directly to stdout)
        #[arg(long)]
        no_pager: bool,
    },
    /// Delete an archive and all its indexed content
    DeleteArchive {
        /// Archive index (from list-archives) or name
        target: String,
    },

    // === Session Linking ===
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
    let db = KsmDatabase::from_config().context("Failed to initialise database")?;

    // TODO(0.3.0): Remove JSON migration - all users should be on SQLite by then
    // One-time migration from JSON (idempotent -- skips if JSON doesn't exist
    // or if metadata table already has data)
    let existing_count = db.load_all_metadata()?.len();

    if existing_count == 0 {
        match db.migrate_from_json() {
            Ok(migrated) if migrated > 0 => {
                println!(
                    "✓ Migrated {} metadata entries from JSON to SQLite",
                    migrated
                );
            }
            Ok(_) => {} // No JSON file or empty, silent
            Err(e) => {
                eprintln!("⚠ Warning: Failed to migrate from JSON: {}", e);
                eprintln!("⚠ Continuing with empty metadata store");
            }
        }
    }

    // Process any pending reindex from previous session
    let reindex_result = archive::process_pending_reindex(&source, &db)?;
    if let Some(warning) = reindex_result.warning {
        eprintln!(
            "⚠ Failed to update search index for \"{}\": {}",
            reindex_result.session_name.unwrap_or_default(),
            warning
        );
        eprintln!("  Run 'ksm reindex' to retry");
    }

    let directory = std::env::current_dir()
        .context("Failed to get current directory")?
        .to_string_lossy()
        .to_string();

    match cli.command {
        Commands::List {
            show_parents,
            compare_methods,
        } => {
            if compare_methods {
                cmd_compare_methods()?;
            } else {
                cmd_list(&source, &db, show_parents, &directory)?;
            }
        }
        Commands::Delete { indices, yes } => {
            cmd_delete(&source, &db, indices, yes, &directory)?;
        }
        Commands::Name { index, name, chain } => {
            cmd_name(&source, &db, index, &name, chain, &directory)?;
        }
        Commands::Tag { index, tags, chain } => {
            cmd_tag(&source, &db, index, &tags, chain, &directory)?;
        }
        Commands::Untag { index, tags, chain } => {
            cmd_untag(&source, &db, index, &tags, chain, &directory)?;
        }
        Commands::CleanMetadata => {
            cmd_clean_metadata(&source, &db, &directory)?;
        }
        Commands::Resume {
            index,
            last,
            tag,
            name,
        } => {
            cmd_resume(&source, &db, index, last, tag, name, &directory)?;
        }
        Commands::Link {
            child_index,
            parent_index,
        } => {
            cmd_link(&source, &db, child_index, parent_index, &directory)?;
        }
        Commands::Unlink { index, keep } => {
            cmd_unlink(&source, &db, index, keep, &directory)?;
        }
        Commands::DetectLinks { force } => {
            cmd_detect_links(&source, &db, force, &directory)?;
        }
        Commands::Archive { index, name, tags } => {
            cmd_archive(&source, &db, index, name, tags, &directory)?;
        }
        Commands::Index { index, name, tags } => {
            cmd_index(&source, &db, index, name, tags, &directory)?;
        }
        Commands::Reindex { index } => {
            cmd_reindex(&source, &db, index, &directory)?;
        }
        Commands::Unindex { index } => {
            cmd_unindex(&source, &db, index, &directory)?;
        }
        Commands::Search {
            query,
            limit,
            expand,
            no_pager,
        } => {
            cmd_search(&db, &query, limit, expand, no_pager, &directory)?;
        }
        Commands::ListArchives => {
            cmd_list_archives(&db, &directory)?;
        }
        Commands::DeleteArchive { target } => {
            cmd_delete_archive(&db, &target, &directory)?;
        }
        Commands::ShowArchive {
            target,
            exchange,
            no_pager,
        } => {
            cmd_show_archive(&db, &target, exchange, no_pager, &directory)?;
        }
    }

    Ok(())
}

// --- Command handlers ---

fn cmd_list(
    source: &dyn SessionSource,
    db: &KsmDatabase,
    show_parents: bool,
    directory: &str,
) -> Result<()> {
    let result = sessions::session_context(source, db, directory)?;

    if result.all_sessions.is_empty() {
        println!("No sessions found.");
        return Ok(());
    }

    let mut notices = Vec::new();

    if result.auto_linked > 0 {
        notices.push(Notice::success(&format!(
            "Auto-linked {} compacted session(s) to their parents.",
            result.auto_linked
        )));
    }

    if !result.invalid_tag_warnings.is_empty() {
        let mut lines = Vec::new();
        for (idx, tag) in &result.invalid_tag_warnings {
            lines.push(format!(
                "{} {}",
                styles::index(*idx),
                styles::tags(std::slice::from_ref(tag))
            ));
        }
        lines.push(
            styles::bold("Tags must be lowercase a-z, 0-9, hyphens, and underscores only.")
                .to_string(),
        );
        lines.push(format!(
            "Remove with: {}",
            styles::hint("ksm untag <index> \"tag\"")
        ));
        notices.push(Notice::warning(
            &format!(
                "{} tag(s) are no longer valid due to updated tag rules:",
                result.invalid_tag_warnings.len()
            ),
            lines,
        ));
    }

    let notice_output = render_notices(&notices);
    if !notice_output.is_empty() {
        print!("{}", notice_output);
        println!();
    }

    let visible = sessions::visible_session_indices(&result.all_sessions, &result.metadata);
    display::print_session_list(
        &result.all_sessions,
        &result.metadata,
        &visible,
        &result.indexed_session_ids,
        show_parents,
    );

    Ok(())
}

fn cmd_name(
    source: &dyn SessionSource,
    db: &KsmDatabase,
    index: usize,
    name: &str,
    apply_to_chain: bool,
    directory: &str,
) -> Result<()> {
    let SessionContext {
        all_sessions,
        metadata: mut meta,
        auto_linked: _,
        ..
    } = sessions::session_context(source, db, directory)?;

    sessions::validate_index(index, all_sessions.len())?;
    let session_id = all_sessions[index].id.clone();

    let scope = if apply_to_chain {
        metadata::MetadataScope::Chain(session_id)
    } else {
        metadata::MetadataScope::Single(session_id)
    };

    let result = metadata::set_name(scope, name, &all_sessions, &mut meta, db)?;

    if result.affected_ids.len() > 1 {
        println!(
            "{}",
            styles::success(&format!(
                "Set name for {} sessions: {}",
                result.affected_ids.len(),
                name
            ))
        );
    } else {
        println!(
            "{}",
            styles::success(&format!("Set name for [{}]: {}", index, name))
        );
    }

    Ok(())
}

fn cmd_tag(
    source: &dyn SessionSource,
    db: &KsmDatabase,
    index: usize,
    tags: &[String],
    apply_to_chain: bool,
    directory: &str,
) -> Result<()> {
    let SessionContext {
        all_sessions,
        metadata: mut meta,
        auto_linked: _,
        ..
    } = sessions::session_context(source, db, directory)?;

    sessions::validate_index(index, all_sessions.len())?;
    let session_id = all_sessions[index].id.clone();

    let validated = metadata::validate_tags(tags)?;

    let scope = if apply_to_chain {
        metadata::MetadataScope::Chain(session_id)
    } else {
        metadata::MetadataScope::Single(session_id)
    };

    let result = metadata::add_tags(scope, &validated, &all_sessions, &mut meta, db)?;

    if result.affected_ids.len() > 1 {
        println!(
            "{}",
            styles::success(&format!(
                "Added tags to {} sessions: {}",
                result.affected_ids.len(),
                validated.join(", ")
            ))
        );
    } else {
        println!(
            "{}",
            styles::success(&format!(
                "Added tags to [{}]: {}",
                index,
                validated.join(", ")
            ))
        );
    }

    Ok(())
}

fn cmd_untag(
    source: &dyn SessionSource,
    db: &KsmDatabase,
    index: usize,
    tags: &[String],
    apply_to_chain: bool,
    directory: &str,
) -> Result<()> {
    let SessionContext {
        all_sessions,
        metadata: mut meta,
        auto_linked: _,
        ..
    } = sessions::session_context(source, db, directory)?;

    sessions::validate_index(index, all_sessions.len())?;
    let session_id = all_sessions[index].id.clone();

    let scope = if apply_to_chain {
        metadata::MetadataScope::Chain(session_id)
    } else {
        metadata::MetadataScope::Single(session_id)
    };

    let result = metadata::remove_tags(scope, tags, &all_sessions, &mut meta, db)?;

    if result.affected_ids.len() > 1 {
        println!(
            "{}",
            styles::success(&format!(
                "Removed tags from {} sessions: {}",
                result.affected_ids.len(),
                tags.join(", ")
            ))
        );
    } else {
        println!(
            "{}",
            styles::success(&format!(
                "Removed tags from [{}]: {}",
                index,
                tags.join(", ")
            ))
        );
    }

    Ok(())
}

fn cmd_clean_metadata(source: &dyn SessionSource, db: &KsmDatabase, directory: &str) -> Result<()> {
    let SessionContext {
        all_sessions,
        metadata: mut meta,
        auto_linked: _,
        ..
    } = sessions::session_context(source, db, directory)?;

    let stale = metadata::clean_metadata(&all_sessions, &mut meta, db)?;

    if stale.is_empty() {
        println!("No stale metadata found.");
    } else {
        println!(
            "{}",
            styles::success(&format!(
                "Removed metadata for {} deleted session(s):",
                stale.len()
            ))
        );
        for (_, display_name) in &stale {
            println!("  - {}", display_name);
        }
    }

    Ok(())
}

fn cmd_resume(
    source: &dyn SessionSource,
    db: &KsmDatabase,
    index: Option<usize>,
    last: bool,
    tag: Option<String>,
    name: Option<String>,
    directory: &str,
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
        let list_result = sessions::session_context(source, db, directory)?;
        if list_result.all_sessions.is_empty() {
            println!("No sessions found.");
            return Ok(());
        }

        let visible =
            sessions::visible_session_indices(&list_result.all_sessions, &list_result.metadata);
        display::print_session_list(
            &list_result.all_sessions,
            &list_result.metadata,
            &visible,
            &list_result.indexed_session_ids,
            false,
        );

        print!(
            "\nSelect session (0-{}): ",
            list_result.all_sessions.len() - 1
        );
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let idx: usize = input.trim().parse().context("Invalid number")?;

        resume::ResumeTarget::Index(idx)
    };

    let result = resume::resume(target, source, db, directory)?;

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
                anyhow::bail!(
                    "Index {} out of range (max: {})",
                    selection,
                    matches.len() - 1
                );
            }

            let retry_target = resume::ResumeTarget::Index(matches[selection].original_index);
            let retry_result = resume::resume(retry_target, source, db, directory)?;
            if let resume::ResumeResult::Ready { display_name, .. } = retry_result {
                println!("Resuming '{}'...", display_name);
            }
            launch_kiro_resume()?;
        }
    }

    Ok(())
}

fn cmd_link(
    source: &dyn SessionSource,
    db: &KsmDatabase,
    child_index: usize,
    parent_index: usize,
    directory: &str,
) -> Result<()> {
    let SessionContext {
        all_sessions,
        metadata: mut meta,
        auto_linked: _,
        ..
    } = sessions::session_context(source, db, directory)?;

    sessions::validate_index(child_index, all_sessions.len())?;
    sessions::validate_index(parent_index, all_sessions.len())?;

    let child_id = all_sessions[child_index].id.clone();
    let parent_id = all_sessions[parent_index].id.clone();

    let result = match chains::link_sessions(&child_id, &parent_id, false, &mut meta, db) {
        Ok(result) => result,
        Err(crate::error::KsmError::MetadataConflict { .. }) => {
            println!("{}", styles::warning("Session already has metadata:"));
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
                        parent_meta
                            .tags
                            .iter()
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(", ")
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

            chains::link_sessions(&child_id, &parent_id, true, &mut meta, db)?
        }
        Err(e) => return Err(e.into()),
    };

    println!(
        "{}",
        styles::success(&format!(
            "Linked session [{}] to parent [{}]",
            child_index, parent_index
        ))
    );
    if let Some(name) = &result.inherited_name {
        println!(
            "{}",
            styles::success(&format!("Inherited name: \"{}\"", name))
        );
    }
    if !result.inherited_tags.is_empty() {
        println!(
            "{}",
            styles::success(&format!(
                "Inherited tags: {}",
                result
                    .inherited_tags
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        );
    }

    Ok(())
}

fn cmd_unlink(
    source: &dyn SessionSource,
    db: &KsmDatabase,
    index: usize,
    keep_metadata: bool,
    directory: &str,
) -> Result<()> {
    let SessionContext {
        all_sessions,
        metadata: mut meta,
        auto_linked: _,
        ..
    } = sessions::session_context(source, db, directory)?;

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

    let result = chains::execute_unlink(session_id, clear, &mut meta, db)?;

    let parent_idx = all_sessions
        .iter()
        .position(|s| s.id == result.former_parent_id);

    if let Some(pidx) = parent_idx {
        println!(
            "{}",
            styles::success(&format!(
                "Unlinked session [{}] from parent [{}]",
                index, pidx
            ))
        );
    } else {
        println!(
            "{}",
            styles::success(&format!("Unlinked session [{}]", index))
        );
    }

    Ok(())
}

fn cmd_detect_links(
    source: &dyn SessionSource,
    db: &KsmDatabase,
    force: bool,
    directory: &str,
) -> Result<()> {
    let SessionContext {
        all_sessions,
        metadata: mut meta,
        indexed_session_ids,
        cache,
        ..
    } = sessions::session_context(source, db, directory)?;

    if all_sessions.is_empty() {
        println!("No sessions found.");
        return Ok(());
    }

    println!("Scanning for compacted sessions...\n");

    let candidates = chains::detect_unlinked_continuations(&all_sessions, &meta, &cache, force)?;

    if candidates.is_empty() {
        println!("No unlinked compacted sessions found.");
        return Ok(());
    }

    for candidate in candidates {
        let child_idx = all_sessions
            .iter()
            .position(|s| s.id == candidate.child.id)
            .unwrap();
        let parent_idx = all_sessions
            .iter()
            .position(|s| s.id == candidate.parent_id)
            .unwrap();
        let parent = &all_sessions[parent_idx];

        let child_name = meta
            .get(&candidate.child.id)
            .and_then(|m| m.name.clone())
            .unwrap_or_else(|| candidate.child.preview.clone());
        let parent_name = meta
            .get(&candidate.parent_id)
            .and_then(|m| m.name.clone())
            .unwrap_or_else(|| parent.preview.clone());

        let child_time = display::format_time_compact(candidate.child.updated_at);
        let parent_time = display::format_time_compact(parent.updated_at);

        println!(
            "[{}] {} ({})",
            child_idx,
            child_name,
            styles::time(&child_time)
        );
        println!("    might continue from");
        println!(
            "[{}] {} ({})",
            parent_idx,
            parent_name,
            styles::time(&parent_time)
        );

        print!("\nLink them? (y/n): ");
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        if input.trim().eq_ignore_ascii_case("y") {
            chains::link_sessions(
                &candidate.child.id,
                &candidate.parent_id,
                true,
                &mut meta,
                db,
            )?;
            println!(
                "{}",
                styles::success(&format!(
                    "Linked [{}] to [{}] \"{}\"",
                    child_idx, parent_idx, parent_name
                ))
            );

            // Hint if parent is indexed
            if indexed_session_ids.contains(&candidate.parent_id) {
                println!(
                    "  [{}] is searchable. To search this compacted session too: ksm index {}",
                    parent_idx, child_idx
                );
            }
            println!();
        } else {
            println!("Skipped.\n");
        }
    }

    Ok(())
}

fn cmd_archive(
    source: &dyn SessionSource,
    db: &KsmDatabase,
    index: usize,
    name_flag: Option<String>,
    tags_flag: Option<Vec<String>>,
    directory: &str,
) -> Result<()> {
    let list_result = sessions::session_context(source, db, directory)?;
    sessions::validate_index(index, list_result.all_sessions.len())?;
    let session = &list_result.all_sessions[index];

    // Block if session has invalid tags
    if let Some(meta) = list_result.metadata.get(&session.id) {
        let invalid: Vec<&String> = meta
            .tags
            .iter()
            .filter(|t| metadata::validate_tag(t).is_err())
            .collect();
        if !invalid.is_empty() {
            eprintln!(
                "Error: Session [{}] has invalid tags that must be fixed before archiving:",
                index
            );
            for tag in &invalid {
                eprintln!("  {}", styles::tags(&[(*tag).clone()]));
            }
            eprintln!("Tags must be lowercase a-z, 0-9, hyphens, and underscores only.");
            eprintln!(
                "Remove with: {}",
                styles::hint(&format!("ksm untag {} \"tag\"", index))
            );
            anyhow::bail!("Fix invalid tags before archiving.");
        }
    }

    println!(
        "Archiving session [{}] \"{}\" ({} messages)",
        index, session.preview, session.msg_count
    );

    let (existing_name, existing_tags) = sessions::get_session_defaults(&session.id, db)?;

    // Resolve name
    let name = if let Some(name) = name_flag {
        name
    } else {
        let default = existing_name.clone().unwrap_or_default();
        print!("Name [{}]: ", default);
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input = input.trim();
        if input.is_empty() {
            if default.is_empty() {
                anyhow::bail!("Archive name is required.");
            }
            default
        } else {
            input.to_string()
        }
    };

    // Resolve tags
    let tags = if let Some(tags) = tags_flag {
        tags
    } else {
        let existing_display = existing_tags.join(" ");
        print!("Tags [{}]: ", existing_display);
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input = input.trim();
        if input.is_empty() {
            existing_tags
        } else {
            input.split_whitespace().map(|s| s.to_string()).collect()
        }
    };

    let result = archive::archive_session(
        &session.id,
        &name,
        tags,
        session.created_at,
        directory,
        source,
        db,
    )?;

    let mut msg = format!("Archived [{}] as '{}'", index, result.archive_name);
    if result.pruned {
        msg.push_str(" [pruned]");
    }
    println!("{}", styles::success(&msg));

    Ok(())
}

/// Index a session (add to search, keep in Kiro).
fn cmd_index(
    source: &dyn SessionSource,
    db: &KsmDatabase,
    index: usize,
    name_flag: Option<String>,
    tags_flag: Option<Vec<String>>,
    directory: &str,
) -> Result<()> {
    let list_result = sessions::session_context(source, db, directory)?;
    sessions::validate_index(index, list_result.all_sessions.len())?;
    let session = &list_result.all_sessions[index];

    // Block if session has invalid tags
    if let Some(meta) = list_result.metadata.get(&session.id) {
        let invalid: Vec<&String> = meta
            .tags
            .iter()
            .filter(|t| metadata::validate_tag(t).is_err())
            .collect();
        if !invalid.is_empty() {
            eprintln!(
                "Error: Session [{}] has invalid tags that must be fixed before archiving:",
                index
            );
            for tag in &invalid {
                eprintln!("  {}", styles::tags(&[(*tag).clone()]));
            }
            eprintln!("Tags must be lowercase a-z, 0-9, hyphens, and underscores only.");
            eprintln!(
                "Remove with: {}",
                styles::hint(&format!("ksm untag {} \"tag\"", index))
            );
            anyhow::bail!("Fix invalid tags before archiving.");
        }
    }

    println!(
        "Indexing session [{}] \"{}\" ({} messages)",
        index, session.preview, session.msg_count
    );

    // Get existing defaults via service
    let (existing_name, existing_tags) = sessions::get_session_defaults(&session.id, db)?;

    // Resolve name
    let name = if let Some(name) = name_flag {
        name
    } else {
        let default = existing_name.clone().unwrap_or_default();
        print!("Name [{}]: ", default);
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input = input.trim();
        if input.is_empty() {
            if default.is_empty() {
                anyhow::bail!("Index name is required.");
            }
            default
        } else {
            input.to_string()
        }
    };

    // Resolve tags
    let tags = if let Some(tags) = tags_flag {
        tags
    } else {
        let existing_display = existing_tags.join(" ");
        print!("Tags [{}]: ", existing_display);
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input = input.trim();
        if input.is_empty() {
            existing_tags
        } else {
            input.split_whitespace().map(|s| s.to_string()).collect()
        }
    };

    let result = archive::index_session(
        &session.id,
        &name,
        tags,
        session.created_at,
        directory,
        source,
        db,
    )?;

    println!(
        "{}",
        styles::success(&format!("Indexed [{}] {}", index, result.archive_name))
    );

    Ok(())
}

/// Reindex sessions.
fn cmd_reindex(
    source: &dyn SessionSource,
    db: &KsmDatabase,
    index: Option<usize>,
    directory: &str,
) -> Result<()> {
    if let Some(idx) = index {
        // Reindex specific session by list index
        let list_result = sessions::session_context(source, db, directory)?;
        sessions::validate_index(idx, list_result.all_sessions.len())?;
        let session = &list_result.all_sessions[idx];

        let result = match archive::reindex_session(&session.id, source, db) {
            Ok(r) => r,
            Err(KsmError::NotIndexed(_)) => {
                println!(
                    "Session [{}] is not indexed. Use 'ksm index {}' to index it.",
                    idx, idx
                );
                return Ok(());
            }
            Err(e) => return Err(e.into()),
        };

        if result.updated {
            println!(
                "{}",
                styles::success(&format!(
                    "Reindexed '{}' ({} -> {} messages)",
                    result.name, result.old_count, result.new_count
                ))
            );
        } else {
            println!("'{}' is already up to date.", result.name);
        }
    } else {
        // Reindex all
        let results = archive::reindex_all(directory, source, db)?;

        if results.is_empty() {
            println!("No indexed sessions found.");
            return Ok(());
        }

        // Print warnings for any failures
        for r in results.iter().filter(|r| r.error.is_some()) {
            eprintln!(
                "⚠ Failed to reindex \"{}\": {}",
                r.name,
                r.error.as_ref().unwrap()
            );
        }

        let updated: Vec<_> = results.iter().filter(|r| r.updated).collect();
        let failed: Vec<_> = results.iter().filter(|r| r.error.is_some()).collect();

        if updated.is_empty() && failed.is_empty() {
            println!("All {} indexed sessions are up to date.", results.len());
        } else {
            for r in &updated {
                println!(
                    "{}",
                    styles::success(&format!(
                        "Reindexed '{}' ({} -> {} messages)",
                        r.name, r.old_count, r.new_count
                    ))
                );
            }
            if !failed.is_empty() {
                eprintln!("\n{} session(s) failed to reindex.", failed.len());
            }
            if !updated.is_empty() {
                println!(
                    "\nUpdated {} of {} indexed sessions.",
                    updated.len(),
                    results.len()
                );
            }
        }
    }

    Ok(())
}

/// Remove index from a session.
fn cmd_unindex(
    source: &dyn SessionSource,
    db: &KsmDatabase,
    index: usize,
    directory: &str,
) -> Result<()> {
    let list_result = sessions::session_context(source, db, directory)?;
    sessions::validate_index(index, list_result.all_sessions.len())?;
    let session = &list_result.all_sessions[index];

    match archive::unindex_session(&session.id, db) {
        Ok(result) => {
            println!(
                "{}",
                styles::success(&format!("Removed index for '{}'", result.name))
            );
        }
        Err(KsmError::NotIndexed(_)) => {
            println!("Session is not indexed.");
        }
        Err(e) => return Err(e.into()),
    }

    Ok(())
}

fn cmd_delete(
    source: &dyn SessionSource,
    db: &KsmDatabase,
    indices: Option<Vec<usize>>,
    skip_confirm: bool,
    directory: &str,
) -> Result<()> {
    let list_result = sessions::session_context(source, db, directory)?;
    let SessionContext {
        all_sessions,
        metadata: mut meta,
        indexed_session_ids,
        ..
    } = list_result;

    let indices = match indices {
        Some(idx) => idx,
        None => {
            // Interactive delete: show sessions, prompt for indices
            println!();
            let visible = sessions::visible_session_indices(&all_sessions, &meta);
            display::print_session_list(
                &all_sessions,
                &meta,
                &visible,
                &indexed_session_ids,
                false,
            );
            println!();
            print!("Enter sessions to delete (comma or space-separated): ");
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

        if let Some(chain_ctx) = metadata::get_chain_context(&session.id, &meta, &all_sessions) {
            // Show chain
            print!("\nSession [{}] is part of a chain: ", idx);
            for (i, chain_id) in chain_ctx.ordered_ids.iter().enumerate() {
                if let Some(chain_idx) = all_sessions.iter().position(|s| &s.id == chain_id) {
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

            let chain_choice = match choice {
                "1" => delete::ChainDeleteChoice::SingleRelink,
                "2" => delete::ChainDeleteChoice::WithParents,
                "3" => delete::ChainDeleteChoice::EntireChain,
                _ => anyhow::bail!("Invalid choice"),
            };

            let result = delete::delete_from_chain(
                &session.id,
                chain_choice,
                &all_sessions,
                &mut meta,
                source,
                db,
            )?;

            let regular_count = result.deleted_ids.len() - result.indexed_count;
            match choice {
                "1" => {
                    println!(
                        "{}",
                        styles::success(&format!("Deleted [{}] and relinked chain", idx))
                    );
                    if result.indexed_count > 0 {
                        println!("{}", styles::success("Search index preserved as archive."));
                    }
                }
                "2" | "3" => {
                    if regular_count > 0 {
                        println!(
                            "{}",
                            styles::success(&format!("Deleted {} session(s)", regular_count))
                        );
                    }
                    if result.indexed_count > 0 {
                        println!(
                            "{}",
                            styles::success(&format!(
                                "Deleted {} indexed session(s). Search index preserved as archive.",
                                result.indexed_count
                            ))
                        );
                    }
                }
                _ => {}
            }
            return Ok(());
        }
    }

    // Standard deletion (no chain or multiple sessions)
    println!("\nSessions to delete:");
    display::print_session_list(&all_sessions, &meta, &indices, &indexed_session_ids, false);

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

    let ids: Vec<String> = indices
        .iter()
        .map(|&i| all_sessions[i].id.clone())
        .collect();
    let result = delete::delete_sessions(&ids, source, &mut meta, db)?;

    let regular_count = result.deleted_ids.len() - result.indexed_count;
    if regular_count > 0 {
        println!(
            "{}",
            styles::success(&format!("Deleted {} session(s)", regular_count))
        );
    }
    if result.indexed_count > 0 {
        println!(
            "{}",
            styles::success(&format!(
                "Deleted {} indexed session(s). Search index preserved as archive.",
                result.indexed_count
            ))
        );
    }

    Ok(())
}

fn cmd_delete_archive(db: &KsmDatabase, target: &str, directory: &str) -> Result<()> {
    // Try parsing as index first
    let result = if let Ok(idx) = target.parse::<usize>() {
        let archive_info = archive::get_archive_by_index(idx, directory, db)?;

        print!(
            "Delete archive '{}' ({} messages)? [y/N] ",
            archive_info.name, archive_info.message_count
        );
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        if input.trim() != "y" && input.trim() != "Y" {
            println!("Cancelled.");
            return Ok(());
        }

        archive::delete_archive_by_index(idx, directory, db)?
    } else {
        // Treat as name
        let archive_info = archive::get_archive_info(target, directory, db)?;

        print!(
            "Delete archive '{}' ({} messages)? [y/N] ",
            target, archive_info.message_count
        );
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        if input.trim() != "y" && input.trim() != "Y" {
            println!("Cancelled.");
            return Ok(());
        }

        archive::delete_archive(target, directory, db)?
    };

    println!(
        "{}",
        styles::success(&format!(
            "Deleted archive '{}' ({} messages)",
            result.archive_name, result.message_count
        ))
    );

    Ok(())
}

fn cmd_search(
    db: &KsmDatabase,
    query: &str,
    limit: u32,
    expand: Option<usize>,
    no_pager: bool,
    directory: &str,
) -> Result<()> {
    let results = archive::search_archives(query, limit, directory, db)?;

    let output = if let Some(n) = expand {
        if n >= results.len() {
            anyhow::bail!(
                "Result index {} out of range (0 to {}).",
                n,
                results.len().saturating_sub(1)
            );
        }
        let chunk = archive::get_expanded_result(
            &results[n].archive_name,
            results[n].exchange_index,
            directory,
            db,
        )?;
        display::format_expanded_exchange(&chunk, &results[n].archive_name)
    } else {
        display::format_search_results(&results)
    };

    if no_pager {
        print!("{}", output);
    } else {
        pager::paged_output(&output)?;
    }

    Ok(())
}

fn cmd_list_archives(db: &KsmDatabase, directory: &str) -> Result<()> {
    let archives = archive::list_archives(directory, db)?;
    display::print_archive_list(&archives);
    Ok(())
}

fn cmd_show_archive(
    db: &KsmDatabase,
    target: &str,
    exchange: Option<i32>,
    no_pager: bool,
    directory: &str,
) -> Result<()> {
    // Try parsing as index first, otherwise treat as name
    let archive_name = if let Ok(idx) = target.parse::<usize>() {
        let archive = archive::get_archive_by_index(idx, directory, db)?;
        archive.name
    } else {
        target.to_string()
    };

    let result = archive::show_archive(&archive_name, directory, db)?;

    if result.chunks.is_empty() {
        return Err(KsmError::EmptyArchive(archive_name).into());
    }

    let output = if let Some(n) = exchange {
        let chunk = result
            .chunks
            .iter()
            .find(|c| c.exchange_index == n)
            .ok_or_else(|| KsmError::ExchangeNotFound {
                index: n,
                archive: archive_name.clone(),
            });
        match chunk {
            Ok(c) => display::format_single_exchange(&result.archive, c),
            Err(KsmError::ExchangeNotFound { index, .. }) => {
                return Err(anyhow::anyhow!(
                    "Exchange {} not found. Archive has {} exchanges (0 to {}).",
                    index,
                    result.chunks.len(),
                    result.chunks.len() - 1
                ));
            }
            Err(e) => return Err(e.into()),
        }
    } else {
        display::format_full_archive(&result.archive, &result.chunks)
    };

    if no_pager {
        print!("{}", output);
    } else {
        pager::paged_output(&output)?;
    }

    Ok(())
}

/// Compare database and CLI methods (testing only).
fn cmd_compare_methods() -> Result<()> {
    use crate::data::{KiroCliSource, KiroDatabase};

    eprintln!("=== Comparing Database vs CLI Methods ===\n");

    let db_source = KiroDatabase::new();
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
                if current_idx.is_some() {
                    eprintln!();
                }
                eprintln!("Session [{}] differences:", diff.index);
                current_idx = Some(diff.index);
            }
            eprintln!(
                "  {}: DB='{}' vs CLI='{}'",
                diff.field, diff.source_a, diff.source_b
            );
        }
        let session_count = result
            .differences
            .iter()
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
