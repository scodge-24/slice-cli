# Agent Workflow Reference

## Overview

AI agents interact with docs through two channels:

- **Slice CLI** for code-aware queries: "what docs are affected?", "what's stale?", "mark as reviewed"
- **Direct file access** for content: reading, editing, creating docs in the vault

Obsidian CLI/API is never required. All agent workflows work headlessly.

## Core Workflows

### 1. "I changed code — what docs need attention?"

The most common agent workflow. Run after modifying source files.

```bash
slice affected-docs src/auth/middleware.py src/auth/sessions.py --json
```

Response:
```json
[
  {
    "doc_id": "auth-guide",
    "path": "wiki/guides/auth-guide.md",
    "matching_slices": ["auth-service"],
    "status": "stale",
    "changed_files": ["src/auth/middleware.py"]
  }
]
```

Exit code: 0 if no affected docs are stale, 1 if any affected doc is stale.

**Agent action**: Read stale docs, decide if they need updating, and update the content when needed. Commit the source/doc changes before stamping; `slice stamp` records the current `HEAD` SHA and refuses to stamp docs whose tracked source files are still uncommitted.

### 2. "What docs cover this slice?"

Before working on a slice, find relevant design context.

```bash
slice docs rust-abc-types
```

Output:
```
[ok   ] boundary-contract-spec  (verified: abc123)  [boundary, contracts]
[STALE] replay-trace-refactor   (verified: def456)  [replay, trace]
```

```bash
slice docs rust-abc-types --json
```

```json
[
  {
    "doc_id": "boundary-contract-spec",
    "path": "wiki/architecture/boundary-contract-spec.md",
    "verified_at": "abc123",
    "tags": ["boundary", "contracts"],
    "stale": false
  }
]
```

**Agent action**: Read the listed docs for design context before making changes.

### 3. "What's stale across the whole repo?"

Periodic check or CI integration.

```bash
slice stale-docs --json
```

```json
[
  {
    "doc_id": "auth-guide",
    "path": "wiki/guides/auth-guide.md",
    "verified_at": "b6cf05a",
    "affected_slices": ["auth-service"],
    "changed_files": ["src/auth/middleware.py"]
  }
]
```

Exit code: 0 if all current, 1 if any stale.

### 4. "I've reviewed/updated a doc — mark it current"

After verifying a doc is accurate:

```bash
# Stamp a specific doc
slice stamp auth-guide

# Stamp all docs for a slice
slice stamp --slice auth-service

# Stamp all stale docs (after bulk review)
slice stamp --all
```

Stamp updates `verified_at` in DOCS.yaml to the current HEAD SHA. It refuses to stamp a doc while that doc's tracked source files are staged or unstaged against `HEAD`, because such a stamp would point at the wrong commit.

### 5. "Find docs about a topic"

Headless content search:

```bash
# Search doc content with ripgrep
rg "boundary contract" wiki/

# Search via slice CLI (searches manifest tags + slice metadata)
slice find boundary
```

With Obsidian running (optional):
```bash
obsidian search query="boundary contract" --json
```

### 6. "Add a new doc to tracking"

Agent creates a doc and registers it:

```bash
# Create the doc in the vault
cat > wiki/guides/new-feature.md << 'EOF'
---
doc_id: new-feature-guide
title: New Feature Guide
kind: guide
status: draft
tags: [feature, guide]
summary: How to use the new feature.
---

# New Feature Guide

Content here.
EOF

# Register in manifest (future: slice doc-add command)
# For now, agents edit DOCS.yaml directly or use a helper
```

## Command Reference

### Staleness queries

| Command | Purpose | Exit code |
|---|---|---|
| `slice stale-docs` | List all stale docs | 0=clean, 1=stale |
| `slice stale-docs --json` | Machine-readable stale list | 0=clean, 1=stale |
| `slice docs <slice-id>` | Docs for a slice with staleness | 0 |
| `slice docs <slice-id> --json` | Machine-readable | 0 |
| `slice affected-docs <path>...` | Docs affected by file changes | 0=no stale affected docs, 1=stale affected docs |
| `slice affected-docs <path>... --json` | Machine-readable | 0=no stale affected docs, 1=stale affected docs |

### Stamping

| Command | Purpose |
|---|---|
| `slice stamp <doc-id>` | Stamp one doc as current |
| `slice stamp --slice <id>` | Stamp all docs for a slice |
| `slice stamp --doc <path>` | Stamp by file path |
| `slice stamp` | Stamp all stale docs |

### Validation

| Command | Purpose |
|---|---|
| `slice check` | Full validation including doc checks |
| `slice check --no-doc-drift` | Skip doc staleness (faster) |
| `slice check --json` | Machine-readable results |

### Search (slice CLI)

| Command | Purpose |
|---|---|
| `slice find <needle>` | Search slice metadata + manifest tags |

## Headless vs Interactive

| Capability | Headless (agent-only) | Interactive (Obsidian available) |
|---|---|---|
| Staleness queries | `slice stale-docs` | same |
| Read doc content | `Read` tool / `cat` | Obsidian app or `obsidian read` |
| Edit doc content | `Edit` tool / direct write | Obsidian app |
| Content search | `rg` / `slice find` | `obsidian search` (JSON) |
| Doc relationships | Read `[[wikilinks]]` from file | Obsidian graph view |
| Tag browsing | `slice find <tag>` | Obsidian tag pane |
| Stamp | `slice stamp` | same |

## Integration Points

### Pre-commit hook (optional)

```bash
# .claude/settings.json hook or git pre-commit
slice stale-docs --json | jq -e 'length == 0'
```

Fails if any docs are stale. Agents are reminded to review docs before committing.

### CI pipeline

```bash
slice check --json
# Includes doc staleness as warnings
# Policy: fail the build if critical docs are stale, or just report
```

### CLAUDE.md integration

```markdown
<important if="you are modifying source code">
After making code changes, run `slice affected-docs <changed-files> --json`
to check if any design docs need updating. If a doc is stale, read it and
either update the content or stamp it after the relevant source/doc changes are
committed if no content changes are needed.
</important>
```

## Error Handling

| Scenario | Behavior |
|---|---|
| No DOCS.yaml exists | All doc commands return empty/clean — graceful degradation |
| Doc file missing | `slice check` reports error; drift check skips the doc |
| Invalid verified_at SHA | Treated as stale with git error message |
| Unknown slice ID in manifest | `slice check` reports error |
| Vault directory missing | `slice check` reports error for each unresolvable path |
