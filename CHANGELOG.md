# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.5] - 2026-03-12

### Added

- Session caching for improved performance

### Changed

- unindex command now accepts index only, not session name

### Fixed

- Chain detection slow on large session lists
- auto_update setting now preserved during config migration
- ksm list slow on cold/warm start
- Slow startup on cold disk cache (1.5s to 50ms)
- Redundant chain indicator with --show-parents

## [0.2.4] - 2026-03-09

### Added

- ksm index command - index a session for search while keeping it in Kiro
- ksm reindex command - update search index for indexed sessions
- [i] marker in ksm list showing indexed sessions
- Index numbers in ksm list-archives output
- Auto-reindex on next command after resuming an indexed session
- [index] config section with auto_update option
- Schema v3 migration with is_indexed column and state table
- ksm unindex command to remove index from a session

### Changed

- Unified KsmDatabase replaces separate SqliteMetadataStore and ArchiveStore
- Row-level database operations instead of load-all/save-all pattern
- Centralised styles module for consistent colours across all commands
- Unified colour scheme across ksm list, list-archives, and search
- ksm archive now deletes session from Kiro after indexing
- ksm delete on indexed session converts to archived, preserving search index
- ksm list display redesigned with fixed columns and compact time format
- ksm list-archives display redesigned to match ksm list format
- CLI help reordered into logical groups
- Services receive directory as parameter instead of calling current_dir() internally
- Documented -V short flag for version in INSTALLATION.md

### Fixed

- Search queries with punctuation no longer cause FTS5 syntax errors
- show-archive now accepts index number from list-archives as well as archive name
- pending_reindex flag now cleared when auto_update config is disabled

## [0.2.3] - 2026-03-03

### Added

- Auto-pager for search results and show-archive using less
- no-pager flag to bypass pager on search and show-archive

### Changed

- Search --limit default raised from 10 to 50

### Fixed

- Documented --limit flag on ksm search in USAGE.md (default: 10)
- Documented FTS5 query syntax with shell quoting examples and Porter stemming behaviour in USAGE.md
- Indented multi-line content in search results, show-archive, and expanded exchange display
- Changed archive tag prompt display from comma-separated to space-separated to match input format
- Sanitised session preview to single line by collapsing whitespace and newlines
- Added --version flag to CLI using clap version attribute
- Expanded installation guide with GLIBC mismatch note, rustup install instructions, and source tarball alternative

## [0.2.2] - 2026-03-02

### Added

- Session archiving with FTS5 full-text search
- Commands: archive, search, list-archives, show-archive, delete-archive
- Coloured labels (User/Assistant/Tools) in archive display
- Visual separators between exchanges in show-archive and search
- Truncated assistant content in full archive view (10 lines)
- Reverse video highlighting for search term matches

### Changed

- Metadata storage migrated from JSON to SQLite (automatic one-time migration)
- Conversation model refactored to typed enums (ConversationData, UserContent, AssistantContent)
- Archive list formatting: bold names, dim metadata, yellow pruned flag

### Fixed

- is_pruned() false positive: use exact match instead of substring contains, remove tool call args check

## [0.2.1] - 2026-03-01

### Added

- ARM64 build support in GitHub Actions

### Fixed

- Broken documentation links in release notes
- GitHub Action now transforms relative links to absolute URLs for releases

## [0.2.0] - 2026-02-27

### Added

- Library crate (lib.rs) enabling future TUI and library API consumers
- Structured error types (KsmError) with thiserror
- Logging infrastructure with log and env_logger crates

### Changed

- Full architecture restructure: layered design with models, data (traits), services, and CLI modules
- Trait-based data access with SessionSource and MetadataStore abstractions
- Services layer providing complete operations with result structs
- Two-tier error handling: thiserror (library) and anyhow (binary)
- CLI commands refactored to thin wrappers around services

### Fixed

- Incorrect kiro-cli URL in README Development section

## [0.1.13] - 2026-02-27

### Added

- Dual licensing (Apache-2.0 OR GPL-3.0-only) with LICENSE-APACHE and LICENSE-GPL3
- Documentation structure in docs/ (INSTALLATION.md, CONFIGURATION.md, USAGE.md, CONTRIBUTING.md)
- GitHub Actions release workflow for automated binary builds on tag push
- GitHub Issue templates for bug reports and feature requests
- SECURITY.md with vulnerability reporting policy
- README badges for license and release version
- AI development disclosure in README
- auto_clean config option to disable automatic metadata cleanup

### Changed

- README shortened to essentials with links to docs/
- Release notes moved to docs/releases/ and cleaned for public use

### Fixed

- Auto-clean wiping all metadata when database returns no sessions

## [0.1.12] - 2026-02-09

### Changed
- Switched to direct database access for session listing (more reliable, no external sqlite3 dependency)
- Session data now fetched from kiro-cli's SQLite database instead of parsing CLI output
- All database operations now use rusqlite library (embedded SQLite)
- Improved time formatting (proper grammar: "1 hour ago" not "1 hours ago")
- Improved message count formatting (proper pluralization: "1 msg" not "1 msgs")

### Added
- Hybrid fallback: automatically falls back to CLI parsing if database access fails
- Hidden `--compare-methods` flag for testing database vs CLI output

## [0.1.11] - 2026-02-05

### Fixed
- Resume and delete commands now hide parent sessions (matching list behavior)
- Parent sessions no longer appear in interactive pickers
- detect-links command now shows session names and tags in prompt
- clean-metadata command now respects directory isolation (only cleans current directory)

### Technical
- Added `filter_parent_sessions()` helper for consistent filtering across commands
- Added `display_filtered_sessions()` unified display function
- Removed `display_sessions_with_metadata()` (replaced by unified function)
- Added auto-migration for legacy metadata entries without directory field
- Auto-migration saves metadata immediately to ensure persistence

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
