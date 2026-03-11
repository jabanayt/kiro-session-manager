# Contributing

KSM is currently in pre-alpha. Feedback is the most valuable contribution right now.

## Reporting Bugs

If something isn't working correctly, please [open a bug report](https://github.com/jabanayt/kiro-session-manager/issues/new?template=bug_report.md).

Include:
- KSM version
- kiro-cli version
- Your OS/distro
- Steps to reproduce the issue
- Any terminal output or error messages

## Requesting Features

Have an idea for an improvement? [Open a feature request](https://github.com/jabanayt/kiro-session-manager/issues/new?template=feature_request.md).

## Building from Source

If you'd like to test the latest changes:

```bash
git clone https://github.com/jabanayt/kiro-session-manager.git
cd kiro-session-manager
git checkout staging  # Latest (possibly untested) changes
cargo build --release
```

The binary is at `target/release/ksm`.

## Code Contributions

PRs are welcome. Bug reports and feature requests are often more helpful at this stage. If you do submit code:

1. Branch from `staging`, not `main`
2. Run `cargo fmt` and `cargo clippy`. Both must pass with no warnings.
3. Put code in the right layer:

```
src/
├── models/    # Data types (no logic)
├── data/      # Database access (traits + implementations)
├── services/  # Business logic (no printing/user interaction)
└── cli/       # Command handlers (parse args, call services, format output)
```

## License

By contributing, you agree that your contributions will be dual-licensed under Apache-2.0 OR GPL-3.0-only, without any additional terms or conditions.
