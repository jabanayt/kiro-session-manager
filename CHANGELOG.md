# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed
- Resume and delete commands now hide parent sessions (matching list behavior)
- Parent sessions no longer appear in interactive pickers

### Technical
- Added `filter_parent_sessions()` helper for consistent filtering across commands
- Added `display_filtered_sessions()` unified display function
- Removed `display_sessions_with_metadata()` (replaced by unified function)

## [0.1.10] - 2026-02-02

### Added
- **Chain-aware delete:** Three options when deleting chained sessions
  - Delete only selected session (relinks around it)
  - Delete session and all parents
  - Delete entire chain
- **Chain-aware tag/untag:** Option to apply tags to entire chain or single session
- **Name --chain flag:** Apply name to entire chain with confirmation prompt
- Chain display shows ordered arrows ([1] → [2] → [3]) across all commands

### Changed
- Parent tags now visible in `--show-parents` view
- Chain operations default to safer/more convenient options

### Fixed
- Stale metadata no longer included in chain operations

### Technical
- Added `get_full_chain()` helper to find all sessions in a chain
- Added `get_ordered_chain()` helper for consistent chain display
- Added `relink_around_session()` to maintain chain integrity on deletion
- Chain filtering now only includes sessions that actually exist

## [0.1.9] - 2026-02-01

### Changed
- Parent indicator color changed from gray to cyan for better visibility
- Progressive indentation in `--show-parents` view for clearer hierarchy
- Inline parent indicator hidden when showing full parent chain

### Technical
- Added `show_parent_inline` parameter to `format_session_display()`

## [0.1.8] - 2026-02-01

### Changed
- **Improved parent detection:** Use message_id overlap instead of timestamp matching
- Parent detection now uses definitive proof (shared message_ids) rather than timing heuristics
- Detection correctly handles long chains by picking most recently created parent
- Added `--force` flag to `ksm detect-links` to override manually_unlinked sessions

### Technical
- Refactored database queries with shared `query_db()` helper function
- Added `get_message_ids()` to extract message_ids from session history
- Parent detection uses `created_at` instead of `updated_at` for chain accuracy
- Timestamp matching retained as fallback for edge cases

## [0.1.7] - 2026-02-01

### Added
- **Session continuation tracking:** Preserve metadata across Kiro compaction events
- `ksm link <child> <parent>` - Manually link child session to parent
- `ksm unlink <index>` - Remove parent relationship (with `--keep` flag to preserve metadata)
- `ksm detect-links` - Auto-detect and interactively link compacted sessions
- `--show-parents` flag for `ksm list` - Display full parent chain with details
- Auto-detection of compacted sessions using Kiro's Compact tag
- Config option `auto_detect_continuations` to enable/disable auto-linking (default: false)
- Database helper module (`src/database.rs`) for querying Kiro's SQLite database
- Parent sessions now hidden from default list view (shown with `--show-parents`)

### Changed
- Session list now filters out parent sessions by default
- Child sessions display inline parent indicator: `↳ from [X]`
- Link command skips warning if child metadata matches parent
- Config file automatically migrates to include new fields

### Technical
- Added `parent_session_id` field to `SessionMetadata`
- Added `manually_unlinked` flag to prevent auto-detection from re-linking
- Implemented linear chain validation (one parent, one child per session)
- Database queries for Compact tag detection and timestamp matching
- Shared detection logic between `list` and `detect-links` commands
- Added `Clone` derive to `Session` struct

### Documentation
- Created ADR-010 documenting session continuation tracking design
- Updated README with link/unlink/detect-links commands
- Added configuration section for auto-detection toggle

## [0.1.6] - 2026-02-01

### Added
- Configuration file support: `~/.ksm/config.toml` for customizable metadata storage
- Three storage modes: `global` (default), `local` (per-directory), `custom` (user path)
- Directory tracking in metadata to prevent cross-project interference
- `toml` dependency for configuration file parsing

### Fixed
- **Critical bug:** Running `ksm list` in different directories no longer wipes metadata from other projects
- Metadata cleanup now only affects sessions from the current directory
- Added safety check to prevent data loss when kiro-cli fails or returns no sessions

### Changed
- Metadata schema now includes optional `directory` field (backward compatible)
- Cleanup logic is directory-aware and only removes stale entries from current project
- Config file auto-generated on first run with helpful comments

### Technical
- Added `config.rs` module for TOML configuration loading
- Updated `metadata_path()` to support configurable storage locations
- Enhanced `cleanup_stale_metadata()` with directory filtering
- Legacy metadata entries without directory field remain supported

## [0.1.5] - 2026-02-01

### Added
- Interactive delete mode: `ksm delete` shows session list and prompts for selection
- `d` alias for delete command: `ksm d` as shorthand for `ksm delete`

### Changed
- Delete confirmation now displays custom names/tags alongside original session names
- Refactored display formatting into reusable `format_session_display()` helper
- Resume by tag picker now shows all tags consistently with other commands

### Technical
- Extracted session display logic into shared helper function
- Added `include_original` parameter to show both custom and original names
- Unified formatting across list, delete, and resume commands

## [0.1.4] - 2026-01-31

### Added
- Resume command with multiple modes: `ksm resume` or `ksm r`
- Resume by index: `ksm r 0` to resume specific session
- Resume by tag: `ksm r -t work` with picker for multiple matches
- Resume by name: `ksm r -n "Session Name"` for exact match
- Resume last session: `ksm r -l` wraps kiro-cli --resume
- Interactive resume picker with number input

### Technical
- Database timestamp manipulation to control session resume order
- Reuses display logic from list command for consistency
- Requires sqlite3 installed for database operations

## [0.1.3] - 2026-01-31

### Changed
- Refactored codebase into modular structure for better maintainability
- Split single 336-line main.rs into 8 focused modules
- Organized code into: models, storage, kiro integration, and command modules

### Technical
- Created `models.rs` for data structures
- Created `storage.rs` for metadata persistence
- Created `kiro.rs` for kiro-cli integration
- Created `commands/` directory with separate modules for each command

## [0.1.2] - 2026-01-31

### Added
- Custom session naming with `ksm name <index> <name>` command
- Session tagging system with `ksm tag <index> <tags...>` command
- Tag removal with `ksm untag <index> <tags...>` command
- Manual metadata cleanup with `ksm clean-metadata` command
- Automatic silent cleanup of stale metadata on list operations
- Metadata storage in `~/.ksm/metadata.json`

### Changed
- List command now displays custom names and tags when available
- Custom names replace default session preview in list output
- Tags displayed as `[tag]` prefixes before session name/preview

## [0.1.1] - 2026-01-31

### Added
- Deletion confirmation prompt before removing sessions
- `-y` / `--yes` flag to skip confirmation prompt
- Display sessions to be deleted before confirmation

### Changed
- Delete command now shows session details (time and preview) before deletion

## [0.1.0] - 2026-01-31

### Added
- Initial release of Kiro Session Manager (ksm)
- `ksm list` command to display sessions with numbered indices
- `ksm delete` command to remove sessions by index numbers
- Support for comma-separated and space-separated index lists
- ANSI color code parsing from kiro-cli output
- Session information display: time ago, message count, and preview
- README with installation and usage instructions

### Technical
- Built with Rust using clap, regex, and anyhow dependencies
- Parses kiro-cli stderr output for session data
- Wraps kiro-cli commands for simplified workflow
