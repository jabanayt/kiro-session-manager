# Configuration

KSM uses `~/.ksm/config.toml` to configure metadata storage location. The config file is created automatically on first run.

## Storage Modes

### Global (default)

Metadata stored in `~/.ksm/metadata.json`. Sessions are still filtered by current directory.

```toml
metadata_storage = "global"
```

### Local

Metadata stored per-directory in `.kiro/ksm-metadata.json`.

```toml
metadata_storage = "local"
```

### Custom

Metadata stored at a user-specified path.

```toml
metadata_storage = "custom"
custom_path = "/path/to/metadata.json"
```

## Which Mode to Choose

- **Global mode:** All metadata stored in one file, but sessions are still filtered by current directory
- **Local mode:** Isolates metadata per project, prevents cross-project interference
- **Custom mode:** Store metadata wherever you prefer (network drive, etc.)

## Auto-Detection

KSM can automatically detect when Kiro compacts a session and link the new session to its parent.

```toml
# Enable automatic detection of compacted sessions
# Only sessions with Kiro's Compact tag will be auto-linked
auto_detect_continuations = false  # Default: false
```

Set to `true` to enable automatic linking on `ksm list`. Use `ksm detect-links` for manual detection.
