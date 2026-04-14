# Installation Guide

## Option 1: Download Binary (Recommended)

Download the latest release from the [Releases page](https://github.com/jabanayt/kiro-session-manager/releases).

Available binaries:

| File | Platform |
|------|----------|
| `ksm-linux-x86_64.tar.gz` | Linux (Intel/AMD) |
| `ksm-linux-aarch64.tar.gz` | Linux (ARM64) |
| `ksm-macos-aarch64.tar.gz` | macOS (Apple Silicon) |
| `ksm-macos-x86_64.tar.gz` | macOS (Intel) |

Extract and place in your PATH:

```bash
tar xzf ksm-<platform>-<arch>.tar.gz
cp ksm ~/.local/bin/
```

Or run directly without installing:

```bash
tar xzf ksm-<platform>-<arch>.tar.gz
./ksm list
```

Linux only: if the binary fails with a GLIBC version error, your system's C library is older than the build target. Use Option 2 to compile from source instead.

## Option 2: Build from Source

Requires Rust 1.70+. If Rust is not installed, install it with [rustup](https://rustup.rs/):

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
```

Then restart your shell, or run `source ~/.cargo/env` to load Rust into the current session.

Clone the repository and build:

```bash
git clone https://github.com/jabanayt/kiro-session-manager.git
cd kiro-session-manager
cargo build --release
```

Alternatively, download a source tarball from the [Releases page](https://github.com/jabanayt/kiro-session-manager/releases) if git is not available:

```bash
tar xzf kiro-session-manager-<version>.tar.gz
cd kiro-session-manager-<version>
cargo build --release
```

The binary is at `target/release/ksm`. Place it in your PATH:

```bash
cp target/release/ksm ~/.local/bin/
```

## Verifying Installation

```bash
ksm -V        # or --version
```

## Requirements

- Linux (x86_64 or ARM64) or macOS (Apple Silicon or Intel)
- kiro-cli installed and in PATH

## PATH Setup

If `~/.local/bin/` is not in your PATH, add it to your shell profile:

```bash
# Linux: add to ~/.bashrc
# macOS: add to ~/.zshrc or ~/.bash_profile
export PATH="$HOME/.local/bin:$PATH"
```

Then reload your shell:

```bash
source ~/.bashrc          # Linux
source ~/.zshrc           # macOS
```
