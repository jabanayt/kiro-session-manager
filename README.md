# Kiro Session Manager (ksm)

A lightweight CLI tool to manage kiro-cli chat sessions efficiently.

## Problem

Kiro-CLI creates a new session every time you quit, leading to:
- Cluttered session lists
- Two-step deletion process (list → copy ID → delete)
- Unhelpful session names (just the last message)

## Solution

`ksm` provides a simple interface to list, delete, tag, and resume sessions by index numbers.

## Installation

```bash
cargo build --release
sudo cp target/release/ksm /usr/local/bin/
# or
cp target/release/ksm ~/.local/bin/
```

## Usage

### List sessions
```bash
ksm list
```

Output:
```
[0] 2 minutes ago | 10 msgs | [work] [urgent] Project Planning
[1] 5 minutes ago | 1 msgs | example2
[2] 10 minutes ago | 1 msgs | example1
```

### Delete sessions by index
```bash
# Interactive mode - shows list and prompts for selection
ksm delete
ksm d

# Delete single session
ksm delete 1
ksm d 1

# Delete multiple sessions (comma or space separated)
ksm delete 1,2,3
ksm d 1 2 3

# Skip confirmation prompt
ksm delete 1 -y
ksm d 1,2,3 -y
```

### Name and tag sessions
```bash
# Set a custom name for a session
ksm name 0 "Project Planning"

# Add tags to a session
ksm tag 0 work urgent

# Remove tags from a session
ksm untag 0 urgent

# Clean up metadata for deleted sessions
ksm clean-metadata
```

### Resume sessions
```bash
# Interactive picker - shows list, prompts for number
ksm resume
ksm r

# Resume by index
ksm r 0

# Resume most recent session
ksm r -l

# Resume by tag (picker if multiple matches)
ksm r -t work

# Resume by exact name
ksm r -n "Project Planning"
```

### Session continuation tracking
```bash
# Link a child session to its parent (preserves metadata across compaction)
ksm link 1 3

# Unlink a session from its parent
ksm unlink 1

# Unlink but keep inherited metadata
ksm unlink 1 --keep

# Auto-detect and link compacted sessions
ksm detect-links

# Show full parent chain
ksm list --show-parents
```

When Kiro compacts a session (via `/compact` or automatically), it creates a new session. KSM can track these parent-child relationships to preserve your names and tags across compactions.

**Features:**
- Automatic detection of compacted sessions (via Kiro's Compact tag)
- Manual linking with `ksm link`
- Parent sessions hidden from default list view
- Full lineage visible with `--show-parents`
- Metadata inheritance from parent to child

## Requirements

- Rust 1.70+
- kiro-cli installed and in PATH
- sqlite3 installed (for resume and session tracking functionality)

## Configuration

KSM uses `~/.ksm/config.toml` to configure metadata storage location. The config file is created automatically on first run.

### Storage Modes

**Global (default):** Metadata stored in `~/.ksm/metadata.json`, shared across all projects
```toml
metadata_storage = "global"
```

**Local:** Metadata stored per-directory in `.kiro/ksm-metadata.json`
```toml
metadata_storage = "local"
```

**Custom:** Metadata stored at a custom path
```toml
metadata_storage = "custom"
custom_path = "/path/to/metadata.json"
```

### Why Configure Storage?

- **Global mode:** Convenient for managing all sessions in one place
- **Local mode:** Isolates metadata per project, prevents cross-project interference
- **Custom mode:** Store metadata wherever you prefer (network drive, Dropbox, etc.)

### Auto-Detection

KSM can automatically detect when Kiro compacts a session and link the new session to its parent:

```toml
# Enable automatic detection of compacted sessions
# Only sessions with Kiro's Compact tag will be auto-linked
auto_detect_continuations = false  # Default: false
```

Set to `true` to enable automatic linking on `ksm list`. Use `ksm detect-links` for manual detection.

## Dependencies

- `clap` - CLI argument parsing
- `regex` - Parse kiro-cli output
- `anyhow` - Error handling
- `serde` + `serde_json` - Metadata serialization
- `toml` - Configuration file parsing
