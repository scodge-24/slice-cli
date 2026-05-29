# Migration Guide: Adding Doc Tracking to an Existing Repo

## Prerequisites

- A repo with `slices/` directory and existing slice definitions
- Design docs with YAML frontmatter (typically in `docs/` or similar)
- The slice CLI installed (`slices_cli.py`)

## Phase 1: Set up the vault

### Create the vault directory

```bash
mkdir wiki/
```

Or designate an existing directory (e.g., `rust/docs/`) as the vault. Any directory of `.md` files works.

### Optional: Install Obsidian

Obsidian is not required but adds graph view, search, and wikilink navigation. On WSL2:

```bash
curl -Lo /tmp/obsidian.deb "https://github.com/obsidianmd/obsidian-releases/releases/download/v1.8.9/obsidian_1.8.9_amd64.deb"
sudo apt install /tmp/obsidian.deb
obsidian --no-sandbox &
```

Open the vault directory in Obsidian.

### Gitignore Obsidian workspace files

```gitignore
# .gitignore (add to repo root or vault directory)
wiki/.obsidian/workspace.json
wiki/.obsidian/workspaces.json
wiki/.obsidian/workspace-mobile.json
```

Keep `wiki/.obsidian/app.json` and `wiki/.obsidian/core-plugins.json` in git so vault settings are shared.

## Phase 2: Add doc_id to existing docs

Each tracked doc needs a `doc_id` in its frontmatter. This is the stable key that survives file renames.

### Before

```yaml
---
version: "0.3"
status: active
tracks:
  - rust/crates/abc_types/src/state.rs
  - rust/crates/abc_funcs/src/_528095a7339f/types.rs
---
```

### After

```yaml
---
doc_id: boundary-contract-spec
title: Boundary Contract Specification
kind: design
status: active
tags: [boundary, contracts, ownership]
summary: Ownership rules and naming invariants for the Rust pipeline.
---
```

Changes:
- Added `doc_id`, `title`, `kind`, `tags`, `summary`
- Removed `tracks:` (migrated to DOCS.yaml)
- Removed `version:` (use git history instead)

### Naming convention for doc_id

Use the filename stem by default: `boundary-contract-spec.md` → `boundary-contract-spec`. Keep it kebab-case, human-readable, and immutable.

## Phase 3: Bootstrap the manifest

### Manual approach

Create `slices/DOCS.yaml`:

```yaml
vault_root: ../wiki

docs:
  boundary-contract-spec:
    path: boundary-contract-spec.md
    slices: [rust-abc-types, rust-abc-funcs-derivative, rust-abc-props]
    verified_at: null
    tags: [boundary, contracts]
```

For each doc:
1. Read its existing `tracks:` field
2. Run `slice for <path>` for each tracked file to resolve slice IDs
3. Add the entry to DOCS.yaml with `verified_at: null`

### Automated bootstrap (future)

```bash
slice docs-bootstrap --vault wiki/ --scan-tracks
```

This command (planned) would:
1. Scan vault for `.md` files with `doc_id` frontmatter
2. Read any existing `tracks:` fields
3. Resolve tracked paths to slice IDs via `slice for`
4. Generate DOCS.yaml with `verified_at: null`
5. Report unresolvable paths and unmapped docs

### Setting verified_at

After bootstrap, all docs start as "unverified" (`verified_at: null`). To set the initial baseline:

```bash
# Stamp all docs as current at HEAD (trusts current state)
slice stamp

# Or stamp individually after review
slice stamp boundary-contract-spec
```

## Phase 4: Add wikilinks (optional)

Convert cross-references in docs to Obsidian wikilinks:

### Before

```markdown
See the boundary contract spec for ownership rules.
The replay harness (documented in mcd_boundary_replay.md) handles...
```

### After

```markdown
See [[boundary-contract-spec]] for ownership rules.
The replay harness (documented in [[mcd-boundary-replay]]) handles...
```

This enables Obsidian's graph view and backlink navigation. Not required for staleness detection — purely for human/agent navigation.

## Phase 5: Verify

```bash
# Check everything is wired up
slice check

# See which docs are stale
slice stale-docs

# See docs for a specific slice
slice docs rust-abc-types

# Test the agent workflow
slice affected-docs src/auth/middleware.py
```

## Phased Migration Strategy

You don't have to migrate all docs at once. The manifest handles mixed states cleanly.

| Wave | Docs | Criteria |
|---|---|---|
| 1 | High-churn design specs | Currently going stale, actively referenced by agents |
| 2 | Process docs, conventions | Moderate churn, govern how work is done |
| 3 | Static reference docs | Rarely change, low staleness risk |
| Skip | Archive docs, one-off reports | Not worth tracking — leave in place or don't add to manifest |

During migration, some docs live in the vault and some don't. The manifest maps wherever they actually are — `path` is just a relative path from `vault_root`.

## Rollback

To remove doc tracking without affecting anything else:
1. Delete `slices/DOCS.yaml`
2. The slice CLI gracefully handles missing manifests (no doc checks)
3. Doc frontmatter (`doc_id`, `tags`, etc.) is harmless — leave or remove
4. Obsidian vault continues working independently
