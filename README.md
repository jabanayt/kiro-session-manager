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
# Delete single session
ksm delete 1

# Delete multiple sessions (comma or space separated)
ksm delete 1,2,3
ksm delete 1 2 3

# Skip confirmation prompt
ksm delete 1 -y
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

## Requirements

- Rust 1.70+
- kiro-cli installed and in PATH
- sqlite3 installed (for resume functionality)

## Dependencies

- `clap` - CLI argument parsing
- `regex` - Parse kiro-cli output
- `anyhow` - Error handling
- `serde` + `serde_json` - Metadata serialization
