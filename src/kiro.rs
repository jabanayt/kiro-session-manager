use anyhow::{Context, Result};
use regex::Regex;
use std::process::Command;

use crate::models::Session;

pub fn get_sessions() -> Result<Vec<Session>> {
    let output = Command::new("kiro-cli")
        .args(["chat", "--list-sessions"])
        .output()
        .context("Failed to execute kiro-cli")?;

    let stderr = String::from_utf8_lossy(&output.stderr);
    parse_sessions(&stderr)
}

pub fn parse_sessions(output: &str) -> Result<Vec<Session>> {
    let re = Regex::new(
        r"Chat SessionId: \x1B\[38;5;\d+m([a-f0-9-]+)\n\x1B\[0m\s+\x1B\[2m(.+?)\x1B\[0m \| (.+?) \| \x1B\[2m(\d+ msgs?)\x1B\[0m"
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
