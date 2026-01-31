# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
