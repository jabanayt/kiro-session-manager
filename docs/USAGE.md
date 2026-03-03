# Usage Guide

## Commands Overview

| Command | Alias | Description |
|---------|-------|-------------|
| `ksm list` | | List all chat sessions with numbered indices |
| `ksm delete` | `ksm d` | Delete sessions by index numbers |
| `ksm name` | | Set a custom name for a session |
| `ksm tag` | | Add tags to a session |
| `ksm untag` | | Remove tags from a session |
| `ksm resume` | `ksm r` | Resume a chat session |
| `ksm link` | | Link a child session to a parent session |
| `ksm unlink` | | Unlink a child session from its parent |
| `ksm detect-links` | | Auto-detect and link compacted sessions |
| `ksm clean-metadata` | | Clean up metadata for deleted sessions |
| `ksm archive` | | Archive a session for search |
| `ksm search` | | Search archived sessions |
| `ksm list-archives` | | List all archives |
| `ksm show-archive` | | View an archived conversation |
| `ksm delete-archive` | | Delete an archive |

## Listing Sessions

```bash
ksm list
```

Output:

```
[0] 2 minutes ago | 10 msgs | [work] [urgent] Project Planning ↳ from [3]
[1] 5 minutes ago | 1 msgs | example2
[2] 10 minutes ago | 1 msgs | example1
```

Each line shows:
- Index number (used by other commands)
- Time since last activity
- Message count
- Tags in brackets, custom name, and parent link indicator

### Show Full Parent Chain

```bash
ksm list --show-parents
```

Output:

```
[0] 2 minutes ago | 10 msgs | [work] [urgent] Project Planning
    ↳ from [3] "Initial Planning" (1 hour ago)
        ↳ from [5] "Project Start" (2 hours ago)
[1] 5 minutes ago | 1 msgs | example2
```

Parent sessions are hidden from the default list view. Use `--show-parents` to see the full lineage with progressive indentation.

## Deleting Sessions

### Interactive Mode

```bash
ksm delete
ksm d
```

Shows the session list and prompts for selection.

### Delete by Index

```bash
# Single session
ksm d 1

# Multiple sessions (comma-separated)
ksm d 1,2,3

# Multiple sessions (space-separated)
ksm d 1 2 3
```

### Skip Confirmation

```bash
ksm d 1 -y
ksm d 1,2,3 --yes
```

### Chain-Aware Deletion

When deleting a session that's part of a chain, you'll be offered three options:

1. **Delete only the selected session** — relinks around it to maintain the chain
2. **Delete the session and all its parents** — removes upstream history
3. **Delete the entire chain** — removes all linked sessions

## Naming Sessions

```bash
ksm name <index> "<name>"
```

Example:

```bash
ksm name 0 "Project Planning"
```

Names appear in the list output, replacing the default message preview.

### Apply to Entire Chain

```bash
ksm name 0 "Project Planning" --chain
```

Applies the name to the selected session and all sessions in its chain.

## Tagging Sessions

### Add Tags

```bash
ksm tag <index> <tags...>
```

Example:

```bash
ksm tag 0 work urgent
```

Tags appear in brackets in the list output: `[work] [urgent] Project Planning`

### Remove Tags

```bash
ksm untag <index> <tags...>
```

Example:

```bash
ksm untag 0 urgent
```

### Chain-Aware Tagging

When tagging or untagging a session that's part of a chain, you'll be offered two options:

1. Apply only to the selected session
2. Apply to entire chain (default)

## Resuming Sessions

### Interactive Picker

```bash
ksm resume
ksm r
```

Shows the session list and prompts for a number.

### Resume by Index

```bash
ksm r 0
```

### Resume Most Recent

```bash
ksm r -l
ksm r --last
```

### Resume by Tag

```bash
ksm r -t work
ksm r --tag work
```

If multiple sessions match the tag, an interactive picker is shown.

### Resume by Name

```bash
ksm r -n "Project Planning"
ksm r --name "Project Planning"
```

Matches the exact session name.

## Session Continuation Tracking

When Kiro compacts a session (via `/compact` or automatically), it creates a new session. KSM can track these parent-child relationships to preserve your names and tags across compactions.

### Link Sessions

```bash
ksm link <child_index> <parent_index>
```

Example:

```bash
ksm link 1 3
```

Links session 1 as a child of session 3. Metadata (name, tags) is inherited from parent to child.

### Unlink Sessions

```bash
ksm unlink <index>
```

Removes the parent link. Inherited metadata (name, tags) is removed.

### Unlink but Keep Metadata

```bash
ksm unlink <index> -k
ksm unlink <index> --keep
```

Removes the parent link but keeps the inherited name and tags.

### Auto-Detect Links

```bash
ksm detect-links
```

Scans sessions for message ID overlap (definitive proof of compaction) and automatically links them.

### Force Detection

```bash
ksm detect-links -f
ksm detect-links --force
```

Re-checks sessions that were previously manually unlinked.

## Metadata Management

### Clean Up Stale Metadata

```bash
ksm clean-metadata
```

Removes metadata entries for sessions that no longer exist (e.g., deleted outside of KSM via kiro-cli directly). Only affects sessions from the current directory.

## Archiving Sessions

Archive valuable sessions to preserve them with full-text search capability.

### Archive a Session

```bash
ksm archive <index>
```

Prompts for a name and optional tags. Or provide them directly:

```bash
ksm archive 0 --name "Project Planning" --tags "work planning"
```

Archives store the full conversation broken into searchable exchanges.

### Search Archives

```bash
ksm search "query terms"
```

Searches across all archives in the current project directory. Matched terms are highlighted in results.

#### Search Options

- `--limit N` -- Maximum number of results to return (default: 10)
- `--expand N` -- Show the full exchange for result N

```bash
# Limit to 5 results
ksm search "query" --limit 5

# View full exchange for result 0
ksm search "query" --expand 0
```

#### FTS5 Query Syntax

Search uses SQLite FTS5 full-text search. Multi-word queries require shell quotes.

```bash
# Single word (no quotes needed)
ksm search prune

# AND -- results must contain both terms
ksm search "prune AND architecture"

# OR -- results contain either term
ksm search "prune OR architecture"

# NOT -- exclude a term
ksm search "prune NOT architecture"

# Prefix -- wildcard matching
ksm search "arch*"

# Exact phrase -- wrap in single quotes around double quotes
ksm search '"context pruning"'
```

Note: Search uses Porter stemming, which means words are reduced to their root form. For example, "prune", "pruning", and "pruned" are all treated as the same term. This means `prune NOT pruning` will return no results because both words share the same stem.

### List Archives

```bash
ksm list-archives
```

Shows all archives for the current project directory with name, message count, age, and tags.

### View an Archive

```bash
# Full conversation (assistant content truncated to 10 lines per exchange)
ksm show-archive "Project Planning"

# Single exchange in full
ksm show-archive "Project Planning" --exchange 3
```

### Delete an Archive

```bash
ksm delete-archive "Project Planning"
```

Prompts for confirmation before deleting the archive and all its indexed content.

## Configuration

See [Configuration](CONFIGURATION.md) for storage modes and settings.
