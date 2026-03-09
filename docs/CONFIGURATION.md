# Configuration

KSM uses `~/.ksm/config.toml` to configure storage location. The config file is created automatically on first run.

Metadata and archives are stored in a SQLite database (`ksm.db`). On first run after upgrading from an older version, existing `metadata.json` data is migrated automatically.

## Storage Modes

### Global (default)

Data stored in `~/.ksm/ksm.db`. Sessions are still filtered by current directory.

```toml
metadata_storage = "global"
```

### Local

Data stored per-directory in `.kiro/ksm.db`.

```toml
metadata_storage = "local"
```

### Custom

Data stored at a user-specified path.

```toml
metadata_storage = "custom"
custom_path = "/path/to/ksm.db"
```

## Which Mode to Choose

- **Global mode:** All data stored in one database, but sessions are still filtered by current directory
- **Local mode:** Isolates data per project, prevents cross-project interference
- **Custom mode:** Store data wherever you prefer (network drive, etc.)

## Auto-Detection

KSM can automatically detect when Kiro compacts a session and link the new session to its parent.

```toml
# Enable automatic detection of compacted sessions
# Only sessions with Kiro's Compact tag will be auto-linked
auto_detect_continuations = false  # Default: false
```

Set to `true` to enable automatic linking on `ksm list`. Use `ksm detect-links` for manual detection.

## Auto-Clean

KSM automatically removes stale metadata entries (for sessions that no longer exist) when running `ksm list` or `ksm resume`.

```toml
# Automatically clean stale metadata entries on list/resume
# Set to false to disable (prevents metadata loss if database fails)
auto_clean = true  # Default: true
```

Set to `false` if you experience database connectivity issues that cause metadata loss. When disabled, use `ksm clean-metadata` to manually remove stale entries.

Even with `auto_clean = true`, KSM will skip cleanup if the session source returns zero sessions, as this typically indicates a database or CLI failure rather than genuinely having no sessions.

## Index Auto-Update

When you resume an indexed session and then quit Kiro, the search index can be automatically updated on your next ksm command.

```toml
[index]
# Automatically update indexed sessions when resumed
# Set to false to require manual 'ksm reindex'
auto_update = true  # Default: true
```

Set to `false` if you prefer to manually control when indexes are updated using `ksm reindex`.
