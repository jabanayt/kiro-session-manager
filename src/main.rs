use anyhow::Result;
use clap::{Parser, Subcommand};

mod commands;
mod kiro;
mod models;
mod storage;

use commands::{delete, list, metadata, resume};

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
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::List => list::list_sessions()?,
        Commands::Delete { indices, yes } => {
            match indices {
                Some(idx) => delete::delete_sessions(Some(idx), yes)?,
                None => delete::delete_sessions(None, yes)?,
            }
        }
        Commands::Name { index, name } => metadata::set_name(index, &name)?,
        Commands::Tag { index, tags } => metadata::add_tags(index, &tags)?,
        Commands::Untag { index, tags } => metadata::remove_tags(index, &tags)?,
        Commands::CleanMetadata => metadata::clean_metadata()?,
        Commands::Resume { index, last, tag, name } => {
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
    }

    Ok(())
}
