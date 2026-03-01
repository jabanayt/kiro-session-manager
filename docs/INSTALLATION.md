# Installation Guide

## Option 1: Download Binary (Recommended)

Download the latest release from the [Releases page](https://github.com/jabanayt/kiro-session-manager/releases).

Extract and place in your PATH:

```bash
tar xzf ksm-linux-<arch>.tar.gz
cp ksm ~/.local/bin/
```

Or run directly without installing:

```bash
tar xzf ksm-linux-<arch>.tar.gz
./ksm list
```

## Option 2: Build from Source

Requires Rust 1.70+.

```bash
git clone https://github.com/jabanayt/kiro-session-manager.git
cd kiro-session-manager
cargo build --release
```

The binary is at `target/release/ksm`. Place it in your PATH:

```bash
cp target/release/ksm ~/.local/bin/
```

## Verifying Installation

```bash
ksm --version
```

## Requirements

- Linux (x86_64 or ARM64)
- kiro-cli installed and in PATH

## PATH Setup

If `~/.local/bin/` is not in your PATH, add it to your shell profile:

```bash
# Add to ~/.bashrc or ~/.zshrc
export PATH="$HOME/.local/bin:$PATH"
```

Then reload your shell:

```bash
source ~/.bashrc
```
