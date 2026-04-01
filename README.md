# Kiro Session Manager (ksm)

[![License](https://img.shields.io/badge/license-Apache--2.0%20OR%20GPL--3.0-blue)](LICENSE-APACHE)
[![Release](https://img.shields.io/github/v/release/jabanayt/kiro-session-manager?include_prereleases)](https://github.com/jabanayt/kiro-session-manager/releases)

A lightweight CLI tool to manage kiro-cli chat sessions efficiently.

## Problem

Kiro-CLI creates a new session every time you quit, leading to:
- Cluttered session lists
- Two-step deletion process (list → copy ID → delete)
- Unhelpful session names (just the first message)

## Solution

`ksm` provides a simple interface to list, delete, tag, resume, archive, and search sessions by index numbers.

## Quick Start

### Installation

Download the latest binary from [Releases](https://github.com/jabanayt/kiro-session-manager/releases), extract, and place in your PATH:

```bash
tar xzf ksm-linux-<arch>.tar.gz
cp ksm ~/.local/bin/
```

Or build from source:

```bash
cargo build --release
cp target/release/ksm ~/.local/bin/
```

See [Installation Guide](docs/INSTALLATION.md) for detailed options.

### Basic Usage

```bash
# List sessions
ksm list

# Delete sessions by index
ksm d 1,2,3

# Resume a session
ksm r 0

# Name a session
ksm name 0 "TUI Testing"

# Tag a session
ksm tag 0 work urgent

# Archive and search sessions
ksm archive 0 --name "planning" --tags "work"
ksm search "planning"
```

See [Usage Guide](docs/USAGE.md) for all commands and features.

## Documentation

- [Installation Guide](docs/INSTALLATION.md)
- [Configuration](docs/CONFIGURATION.md)
- [Usage Guide](docs/USAGE.md)
- [Contributing](docs/CONTRIBUTING.md)
- [Release Notes](docs/releases/)

## Requirements

- Linux (x86_64 or ARM64)
- kiro-cli installed and in PATH

MacOS is not currently supported. The database path is hardcoded to the Linux location. MacOS support is planned for a future release.

## Development

This project is built with AI assistance from [kiro-cli](https://kiro.dev/cli/). As a session manager for kiro-cli, this is a natural fit. All design decisions, code review, and testing are human-led.

Please keep issue discussions focused on bugs and features. Off-topic issues will be closed.

## License

Licensed under either of:
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- GNU General Public License v3.0 ([LICENSE-GPL3](LICENSE-GPL3))

at your option.
