use log::debug;
use regex::Regex;
use std::process::Command;

use crate::data::SessionSource;
use crate::error::{KsmError, Result};
use crate::models::{ConversationData, Session, SourceType};

/// Session source backed by parsing kiro-cli's stderr output.
///
/// Fallback when database is unavailable. Provides degraded data:
/// - Timestamps are approximate (parsed from "X hours ago" strings)
/// - created_at is set to 0 (unknown)
/// - No conversation data access (get_conversation, get_message_ids, etc. return errors)
pub struct KiroCliSource;

impl Default for KiroCliSource {
    fn default() -> Self {
        Self::new()
    }
}

impl KiroCliSource {
    pub fn new() -> Self {
        KiroCliSource
    }

    /// Parse kiro-cli stderr output into sessions.
    fn parse_output(&self, output: &str) -> Result<Vec<Session>> {
        let re = Regex::new(
            r"Chat SessionId: \x1B\[38;5;\d+m([a-f0-9-]+)\n\x1B\[0m\s+\x1B\[2m(.+?)\x1B\[0m \| (.+?) \| \x1B\[2m(\d+) msgs?\x1B\[0m",
        ).map_err(|e| KsmError::KiroCli(format!("Regex error: {}", e)))?;

        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time before UNIX epoch")
            .as_millis() as i64;

        let sessions: Vec<Session> = re
            .captures_iter(output)
            .map(|cap| {
                let time_ago_str = &cap[2];
                let approximate_updated_at = parse_time_ago_to_ms(time_ago_str, now_ms);
                let msg_count: u32 = cap[4].parse().unwrap_or(0);

                Session {
                    id: cap[1].to_string(),
                    created_at: 0, // unknown from CLI output
                    updated_at: approximate_updated_at,
                    preview: cap[3].to_string(),
                    msg_count,
                    source_type: SourceType::Legacy,
                }
            })
            .collect();

        debug!("Parsed {} sessions from CLI output", sessions.len());
        Ok(sessions)
    }
}

/// Best-effort conversion of "X hours ago" to millisecond timestamp.
fn parse_time_ago_to_ms(time_ago: &str, now_ms: i64) -> i64 {
    // Parse patterns: "X seconds ago", "X minutes ago", "X hours ago", "X days ago"
    // Returns approximate timestamp. Defaults to now_ms if unparseable.
    let parts: Vec<&str> = time_ago.split_whitespace().collect();
    if parts.len() >= 2
        && let Ok(n) = parts[0].parse::<i64>()
    {
        let ms = match parts[1] {
            s if s.starts_with("second") => n * 1000,
            s if s.starts_with("minute") => n * 60 * 1000,
            s if s.starts_with("hour") => n * 3600 * 1000,
            s if s.starts_with("day") => n * 86400 * 1000,
            _ => 0,
        };
        return now_ms - ms;
    }
    now_ms
}

impl SessionSource for KiroCliSource {
    fn list_sessions(&self) -> Result<Vec<Session>> {
        let output = Command::new("kiro-cli")
            .args(["chat", "--list-sessions"])
            .output()
            .map_err(|e| KsmError::KiroCli(format!("Failed to execute kiro-cli: {}", e)))?;

        let stderr = String::from_utf8_lossy(&output.stderr);
        self.parse_output(&stderr)
    }

    fn list_session_updates(&self) -> Result<Vec<(String, i64)>> {
        Err(KsmError::KiroCli(
            "Session updates not available via CLI fallback".to_string(),
        ))
    }

    fn get_conversation(
        &self,
        _session_id: &str,
        _source_type: SourceType,
    ) -> Result<ConversationData> {
        Err(KsmError::KiroCli(
            "Conversation data not available via CLI fallback".to_string(),
        ))
    }

    fn get_conversation_with_created_at(
        &self,
        _session_id: &str,
        _source_type: SourceType,
    ) -> Result<(ConversationData, i64)> {
        Err(KsmError::KiroCli(
            "Conversation with timestamps not available via CLI fallback".to_string(),
        ))
    }

    fn get_message_ids(&self, _session_id: &str, _source_type: SourceType) -> Result<Vec<String>> {
        Err(KsmError::KiroCli(
            "Message IDs not available via CLI fallback".to_string(),
        ))
    }

    fn has_compact_tag(&self, _session_id: &str, _source_type: SourceType) -> Result<bool> {
        Err(KsmError::KiroCli(
            "Compact tag check not available via CLI fallback".to_string(),
        ))
    }

    fn get_timestamps(&self, _session_id: &str, _source_type: SourceType) -> Result<(i64, i64)> {
        Err(KsmError::KiroCli(
            "Timestamps not available via CLI fallback".to_string(),
        ))
    }

    fn update_timestamp(
        &self,
        _session_id: &str,
        _timestamp: i64,
        _source_type: SourceType,
    ) -> Result<()> {
        Err(KsmError::KiroCli(
            "Timestamp update not available via CLI fallback".to_string(),
        ))
    }

    fn delete_session(&self, session_id: &str, _source_type: SourceType) -> Result<()> {
        let output = Command::new("kiro-cli")
            .args(["chat", "--delete-session", session_id])
            .output()?;

        if !output.status.success() {
            return Err(KsmError::KiroCli(format!(
                "Failed to delete session {}: {}",
                session_id,
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn test_parse_time_ago_seconds() {
        let now = 1000000;
        assert_eq!(parse_time_ago_to_ms("30 seconds ago", now), 1000000 - 30000);
        assert_eq!(parse_time_ago_to_ms("1 second ago", now), 1000000 - 1000);
    }

    #[test]
    fn test_parse_time_ago_minutes() {
        let now = 1000000;
        assert_eq!(parse_time_ago_to_ms("5 minutes ago", now), 1000000 - 300000);
        assert_eq!(parse_time_ago_to_ms("1 minute ago", now), 1000000 - 60000);
    }

    #[test]
    fn test_parse_time_ago_hours() {
        let now = 10000000;
        assert_eq!(parse_time_ago_to_ms("2 hours ago", now), 10000000 - 7200000);
        assert_eq!(parse_time_ago_to_ms("1 hour ago", now), 10000000 - 3600000);
    }

    #[test]
    fn test_parse_time_ago_days() {
        let now = 100000000;
        assert_eq!(
            parse_time_ago_to_ms("3 days ago", now),
            100000000 - 259200000
        );
        assert_eq!(parse_time_ago_to_ms("1 day ago", now), 100000000 - 86400000);
    }

    #[test]
    fn test_parse_time_ago_invalid_returns_now() {
        let now = 1000000;
        assert_eq!(parse_time_ago_to_ms("invalid", now), now);
        assert_eq!(parse_time_ago_to_ms("", now), now);
        assert_eq!(parse_time_ago_to_ms("xyz ago", now), now);
    }
}
