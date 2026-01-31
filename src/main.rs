use anyhow::Result;
use clap::{Parser, Subcommand};

mod commands;
mod kiro;
mod models;
mod storage;

use commands::{delete, list, metadata};

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

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::List => list::list_sessions()?,
        Commands::Delete { indices, yes } => delete::delete_sessions(&indices, yes)?,
        Commands::Name { index, name } => metadata::set_name(index, &name)?,
        Commands::Tag { index, tags } => metadata::add_tags(index, &tags)?,
        Commands::Untag { index, tags } => metadata::remove_tags(index, &tags)?,
        Commands::CleanMetadata => metadata::clean_metadata()?,
    }

    Ok(())
}
