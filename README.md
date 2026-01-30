# Kiro Session Manager (ksm)

A lightweight CLI tool to manage kiro-cli chat sessions efficiently.

## Problem

Kiro-CLI creates a new session every time you quit, leading to:
- Cluttered session lists
- Two-step deletion process (list → copy ID → delete)
- Unhelpful session names (just the last message)

## Solution

`ksm` provides a simple interface to list and delete sessions by index numbers.

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
[0] 2 minutes ago | 10 msgs | I have an issue with...
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
```

## Requirements

- Rust 1.70+
- kiro-cli installed and in PATH

## Dependencies

- `clap` - CLI argument parsing
- `regex` - Parse kiro-cli output
- `anyhow` - Error handling
