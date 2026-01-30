# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
