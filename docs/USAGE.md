# Usage Guide

## Commands Overview

| Command | Alias | Description |
|---------|-------|-------------|
| `ksm list` | | List all chat sessions with numbered indices |
| `ksm resume` | `ksm r` | Resume a chat session |
| `ksm delete` | `ksm d` | Delete sessions by index numbers |
| `ksm name` | | Set a custom name for a session |
| `ksm tag` | | Add tags to a session |
| `ksm untag` | | Remove tags from a session |
| `ksm clean-metadata` | | Clean up metadata for deleted sessions |
| `ksm index` | | Index a session for search (keeps session in Kiro) |
| `ksm unindex` | | Remove index from a session |
| `ksm reindex` | | Update search index for indexed sessions |
| `ksm search` | | Search indexed and archived sessions |
| `ksm archive` | | Archive a session (index + delete from Kiro) |
| `ksm list-archives` | | List all archives |
| `ksm show-archive` | | View an archived conversation |
| `ksm delete-archive` | | Delete an archive |
| `ksm link` | | Link a child session to a parent session |
| `ksm unlink` | | Unlink a child session from its parent |
| `ksm detect-links` | | Auto-detect and link compacted sessions |

## Listing Sessions

```bash
ksm list
```

Output:

```
[0]        [t]  Project Planning                           2m    10 msgs   work, urgent
[1]   [i]       API Integration Research                  15m    35 msgs   research
[2]             Quick question about Rust                  1h     5 msgs
```

Each line shows:
- Index number (used by other commands)
- `[i]` marker if session is indexed (searchable)
- `[t]` marker if session is a TUI/ACP session (created via `kiro-cli acp` or an editor integration like JetBrains or Zed)
- Session name or first message preview
- Time since last activity (compact format: s, m, h, d, w)
- Message count
- Tags (displayed comma-separated; use spaces to separate when adding tags)

### TUI/ACP Sessions

Sessions created through editor integrations (JetBrains, Zed) or `kiro-cli acp` are stored as file pairs at `~/.kiro/sessions/cli/` rather than in kiro-cli's SQLite database. These sessions are shown with a `[t]` marker and support all the same commands as regular sessions: delete, resume, name, tag, archive, index, and search.

### Show Full Parent Chain

```bash
ksm list --show-parents
```

Output:

```
[0]        Project Planning ↳ [3]                     2m    10 msgs   work, urgent
    ↳ from [3] "Initial Planning" (1h ago)
        ↳ from [5] "Project Start" (2h ago)
[1]   [i]  API Integration Research                  15m    35 msgs   research
```

Parent sessions are hidden from the default list view. Use `--show-parents` to see the full lineage with progressive indentation.

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

1. **Delete only the selected session** - relinks around it to maintain the chain
2. **Delete the session and all its parents** - removes upstream history
3. **Delete the entire chain** - removes all linked sessions

### Deleting Indexed Sessions

When you delete a session that has been indexed, the search index is preserved as an archive. This ensures your searchable content isn't lost.

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

Tags appear in the list output: `work, urgent`

Tags must be lowercase letters, numbers, hyphens, underscores, or dots (e.g. `work`, `bug-fix`, `v0.2.6`). Tags are automatically lowercased on input.

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

## Indexing and Archiving

KSM provides two ways to make sessions searchable:

| Command | Session in Kiro | Searchable | Use Case |
|---------|-----------------|------------|----------|
| `ksm index` | Kept | Yes | Active sessions you want to search |
| `ksm archive` | Deleted | Yes | Completed sessions to preserve |

### Index a Session

Index makes a session searchable while keeping it active in Kiro:

```bash
ksm index <index>
```

Prompts for a name and optional tags. Or provide them directly:

```bash
ksm index 0 --name "Project Planning" --tags "work planning"
```

Indexed sessions show `[i]` in `ksm list`.

### Remove an Index

```bash
ksm unindex <index>
```

Removes the search index. The session remains in Kiro.

### Update Indexes

When you resume an indexed session and add more messages, the index can become outdated.

```bash
# Update all indexed sessions
ksm reindex

# Update specific session
ksm reindex 3
```

By default, indexes are automatically updated when you resume an indexed session. See [Configuration](CONFIGURATION.md) for the `auto_update` setting.

### Archive a Session

Archive indexes a session and then deletes it from Kiro:

```bash
ksm archive <index>
```

Prompts for a name and optional tags. Or provide them directly:

```bash
ksm archive 0 --name "Project Planning" --tags "work planning"
```

Use archive for sessions you're done with but want to preserve for search.

## Searching

Search across all indexed and archived sessions:

```bash
ksm search "query terms"
```

Matched terms are highlighted in results.

### Search Options

- `--limit N` - Maximum number of results (default: 50)
- `--expand N` - Show the full exchange for result N
- `--no-pager` - Disable pager (print directly to stdout)

```bash
# Limit to 5 results
ksm search "query" --limit 5

# View full exchange for result 0
ksm search "query" --expand 0
```

Long output is automatically piped through a pager when it exceeds the terminal height.

### FTS5 Query Syntax

Search uses SQLite FTS5 full-text search. Multi-word queries require shell quotes.

```bash
# Single word (no quotes needed)
ksm search prune

# AND - results must contain both terms
ksm search "prune AND architecture"

# OR - results contain either term
ksm search "prune OR architecture"

# NOT - exclude a term
ksm search "prune NOT architecture"

# Prefix - wildcard matching
ksm search "arch*"

# Exact phrase
ksm search '"context pruning"'
```

Note: Search uses Porter stemming. Words like "prune", "pruning", and "pruned" are treated as the same term.

## Managing Archives

### List Archives

```bash
ksm list-archives
```

Output:

```
[0]  project-planning-session              1d ago    33 msgs   work, planning
[1]  api-research-complete                 3d ago   128 msgs   research, api
```

### View an Archive

```bash
# By name
ksm show-archive "project-planning-session"

# By index from list-archives
ksm show-archive 0

# Single exchange in full
ksm show-archive 0 --exchange 3

# Disable pager
ksm show-archive 0 --no-pager
```

### Delete an Archive

```bash
# By name
ksm delete-archive "project-planning-session"

# By index from list-archives
ksm delete-archive 0
```

Prompts for confirmation before deleting.

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

Removes metadata entries for sessions that no longer exist. Only affects sessions from the current directory.

## Configuration

See [Configuration](CONFIGURATION.md) for storage modes and settings.
