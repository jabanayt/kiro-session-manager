use anyhow::Result;
use clap::{Parser, Subcommand};

mod commands;
mod config;
mod database;
mod kiro;
mod models;
mod storage;

use commands::{delete, detect, link, list, metadata, resume, unlink};

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
    List {
        /// Show full parent chain with details
        #[arg(long)]
        show_parents: bool,
        /// Compare database and CLI methods (testing only)
        #[arg(long, hide = true)]
        compare_methods: bool,
    },
    /// Delete sessions by index numbers (e.g., "1,3,5" or "1 3 5")
    #[command(alias = "d")]
    Delete {
        indices: Option<Vec<usize>>,
        /// Skip confirmation prompt
        #[arg(short, long)]
        yes: bool,
    },
    /// Set a custom name for a session
    Name {
        index: usize,
        name: String,
        /// Apply name to entire chain
        #[arg(long)]
        chain: bool,
    },
    /// Add tags to a session
    Tag { index: usize, tags: Vec<String> },
    /// Remove tags from a session
    Untag { index: usize, tags: Vec<String> },
    /// Clean up metadata for deleted sessions
    CleanMetadata,
    /// Resume a chat session
    #[command(alias = "r")]
    Resume {
        /// Session index to resume
        index: Option<usize>,
        /// Resume most recent session
        #[arg(short, long)]
        last: bool,
        /// Resume by tag
        #[arg(short, long)]
        tag: Option<String>,
        /// Resume by name
        #[arg(short, long)]
        name: Option<String>,
    },
    /// Link a child session to a parent session
    Link {
        /// Child session index
        child_index: usize,
        /// Parent session index
        parent_index: usize,
    },
    /// Unlink a child session from its parent
    Unlink {
        /// Session index to unlink
        index: usize,
        /// Keep inherited metadata (name and tags)
        #[arg(short, long)]
        keep: bool,
    },
    /// Auto-detect and link compacted sessions to their parents
    DetectLinks {
        /// Force detection even for manually unlinked sessions
        #[arg(short, long)]
        force: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::List {
            show_parents,
            compare_methods,
        } => {
            if compare_methods {
                list::compare_methods()?;
            } else {
                list::list_sessions(show_parents)?;
            }
        }
        Commands::Delete { indices, yes } => match indices {
            Some(idx) => delete::delete_sessions(Some(idx), yes)?,
            None => delete::delete_sessions(None, yes)?,
        },
        Commands::Name { index, name, chain } => metadata::set_name(index, &name, chain)?,
        Commands::Tag { index, tags } => metadata::add_tags(index, &tags)?,
        Commands::Untag { index, tags } => metadata::remove_tags(index, &tags)?,
        Commands::CleanMetadata => metadata::clean_metadata()?,
        Commands::Resume {
            index,
            last,
            tag,
            name,
        } => {
            if last {
                resume::resume_last()?;
            } else if let Some(tag) = tag {
                resume::resume_by_tag(&tag)?;
            } else if let Some(name) = name {
                resume::resume_by_name(&name)?;
            } else if let Some(idx) = index {
                resume::resume_session(idx)?;
            } else {
                resume::interactive_resume()?;
            }
        }
        Commands::Link {
            child_index,
            parent_index,
        } => {
            link::link_sessions(child_index, parent_index)?;
        }
        Commands::Unlink { index, keep } => {
            unlink::unlink_session(index, keep)?;
        }
        Commands::DetectLinks { force } => {
            detect::detect_continuations(force)?;
        }
    }

    Ok(())
}
