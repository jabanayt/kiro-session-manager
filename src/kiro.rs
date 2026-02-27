use anyhow::{Context, Result};
use regex::Regex;
use std::process::Command;

use crate::database::fetch_sessions_from_db;
use crate::models::Session;

pub fn get_sessions() -> Result<Vec<Session>> {
    // Try database first
    match fetch_sessions_from_db() {
        Ok(sessions) => Ok(sessions),
        Err(e) => {
            // Warn user about database failure, fall back to CLI
            eprintln!("⚠ Database access failed: {}", e);
            eprintln!("⚠ Falling back to CLI parsing...\n");
            get_sessions_from_cli()
        }
    }
}

fn get_sessions_from_cli() -> Result<Vec<Session>> {
    let output = Command::new("kiro-cli")
        .args(["chat", "--list-sessions"])
        .output()
        .context("Failed to execute kiro-cli")?;

    let stderr = String::from_utf8_lossy(&output.stderr);
    parse_sessions(&stderr)
}

pub fn parse_sessions(output: &str) -> Result<Vec<Session>> {
    let re = Regex::new(
        r"Chat SessionId: \x1B\[38;5;\d+m([a-f0-9-]+)\n\x1B\[0m\s+\x1B\[2m(.+?)\x1B\[0m \| (.+?) \| \x1B\[2m(\d+ msgs?)\x1B\[0m",
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
