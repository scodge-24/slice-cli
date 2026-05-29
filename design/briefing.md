# Design Briefing: Documentation Staleness Tracking for Agent-Assisted Codebases

## The Problem

AI coding agents (Claude Code, Codex) work on large codebases daily. Design docs — boundary specs, refactor plans, architecture guides — are critical context for making correct changes. But these docs go stale silently when code changes without corresponding doc updates. Agents read stale docs, trust them, and make wrong decisions.

We need a system that:
1. Detects when a doc has fallen out of sync with the code it describes
2. Tells agents which docs are affected by a code change
3. Lets agents mark docs as reviewed after updating
4. Doesn't create more maintenance burden than the staleness it prevents

The target repo is a Rust numerical simulation pipeline (~80k LoC, 28 code slices, ~16 active design docs). AI agents work on it daily.

## Existing Infrastructure: Slice CLI

We have a `slice` CLI that manages **code slices** — lightweight ownership metadata for code regions:

```yaml
# slices/auth-service.md (frontmatter)
slice_id: auth-service
description: Authentication and session management
files:
  - src/auth/middleware.py
  - src/auth/sessions.py
dependencies: [data-model]
```

The CLI provides navigation (`slice for <path>`, `slice find`, `slice grep`), dependency queries (`slice deps`), and integrity checks (`slice check`). There are 28 slices covering the full codebase. An `INDEX.md` tracks all slices with a git SHA that flags when the index is stale relative to HEAD.

## Solution: Two-Layer Architecture

### Layer 1: Obsidian Vault (wiki)

Docs live as plain `.md` files with YAML frontmatter in a vault directory. Obsidian (optional) provides graph view, `[[wikilink]]` navigation, search, and tag browsing. The vault works headlessly without Obsidian — agents read files directly.

Doc frontmatter is content-oriented and decoupled from code:

```yaml
---
doc_id: boundary-contract-spec       # stable key, survives renames
title: Boundary Contract Specification
kind: design                         # design | guide | reference | adr
status: active                       # active | draft | archived
tags: [boundary, contracts, ownership]
summary: Ownership rules and naming invariants for the Rust pipeline.
---
```

Doc-to-doc relationships use `[[wikilinks]]` — standard Obsidian markup. No custom hierarchy system needed.

### Layer 2: Slice CLI (code bridge)

The slice CLI owns the mapping between docs and code via `slices/DOCS.yaml`:

```yaml
vault_root: ../wiki

docs:
  boundary-contract-spec:             # keyed by doc_id, not path
    path: architecture/boundary-contract-spec.md
    slices: [rust-abc-types, rust-abc-funcs-derivative, rust-abc-props]
    verified_at: abc123def456          # git SHA when last reviewed
    tags: [boundary, contracts]
    # optional granularity controls:
    include: []                        # narrow to specific files within slices
    exclude: []                        # filter out noisy paths
```

### The Bridge

`DOCS.yaml` is the single source of truth for which docs track which code. It lives in `slices/`, not in the vault — it's code metadata, not doc content.

Key design choice: **keyed by `doc_id`, not file path**. Obsidian users rename and move files. `doc_id` in doc frontmatter is immutable; the manifest stores the current `path` separately.

### Staleness Detection

For each doc in the manifest:
1. Resolve tracked files: union of `files[]` from all linked slices (or `include` override, minus `exclude`)
2. `git diff <verified_at>..HEAD -- <resolved-files>` for committed changes
3. `git diff HEAD -- <resolved-files>` for staged/unstaged changes
4. If any file changed → doc is stale

### CLI Commands

| Command | Purpose |
|---|---|
| `slice stale-docs [--json]` | List all stale docs with affected slices and changed files |
| `slice affected-docs <path>... [--json]` | Given changed files, which docs need attention? Exits 1 only when an affected doc is stale. |
| `slice docs <slice-id> [--json]` | Which docs track this slice? Show staleness. |
| `slice stamp [doc-id] [--slice id] [--doc path]` | Mark doc(s) verified: records a content fingerprint of their tracked files (works on a dirty tree) |
| `slice check` | Full validation including doc drift warnings |

### What Each Layer Owns

| Concern | Owner |
|---|---|
| Doc content, prose, hierarchy | Vault (Obsidian) |
| Doc-to-doc links | Vault (`[[wikilinks]]`) |
| Doc tags, search | Vault (frontmatter + Obsidian) |
| Doc-to-code mapping | Slice CLI (`DOCS.yaml`) |
| Staleness detection | Slice CLI (`git diff`) |
| "What docs are affected?" | Slice CLI (`affected-docs`) |
| "Find docs about X" | Vault (`rg`, Obsidian search) |
| Stamp as reviewed | Slice CLI (`stamp`) |

## Integration: Context Engine

The context engine is a separate system that orchestrates multi-cycle AI agent work. It maintains durable context artifacts (bundle files) across agent sessions, runs deterministic review to decide if work is complete, and spawns fresh agents each cycle with accumulated context.

### Current State

The engine's main loop per cycle:
1. Build task bundle → assemble agent prompt from bundle files
2. Spawn implementation agent (`claude -p`)
3. Ingest session JSONL → normalize events → write to ledger
4. Reduce: extract edited files, validations, failures, user signals
5. Curate: LLM updates bundle files (state.md, rules.md, progress.md, dead_ends.md)
6. Review: deterministic check → `cycle_complete` | `spawn_another` | `escalate`

Slices are explicitly stubbed for v0.2: `slice_ids: []` in task bundles, `awaiting_slice_refresh` phase exists but is unused, `slice-maintainer.md` describes the intended contract.

### Where Doc Tracking Integrates

**Pre-cycle (task bundle enrichment)**:

When building the task bundle, query doc staleness for the task's relevant slices. Include in `context_inputs`:
- Relevant doc paths + summaries
- Staleness warnings: "boundary-contract-spec.md is stale — verify before trusting"

The agent receives design context *and* knows whether to trust it.

**Post-cycle (observation emission)**:

The reducer already extracts `edited_files`. After reduction:
- `slice affected-docs <edited_files>` → identify newly-stale docs
- Emit observation: "auth-guide.md now stale due to edits in middleware.py"
- Persists in the observation store for the reviewer and next cycle

**Curator context**:

The curator can append a doc-staleness section to `state.md`:
```markdown
## Doc Staleness
- boundary-contract-spec.md: STALE (middleware.py changed in cycle 2)
- auth-guide.md: current
```

This persists across cycles — the next agent sees it.

**Reviewer gating (v0.3+)**:

For tasks that touch tracked code, the reviewer could gate on "critical docs updated" as part of the acceptance check. Not a hard gate initially — just a signal that feeds into `spawn_another` vs `escalate` decisions.

### Implementation Order

| Phase | What | Depends on |
|---|---|---|
| v0.2 | Slice integration (fill `slice_ids`, refresh gating) | slice CLI exists (done) |
| v0.2+ | Doc awareness (affected-docs in bundle, staleness warnings) | DOCS.yaml manifest, `affected-docs` command |
| v0.3 | Doc observations (post-cycle emission, curator state.md entries) | Observation store, curator contract |
| v0.3+ | Doc update gating (reviewer checks doc freshness) | Reviewer rule family |

## Key Decisions (ADRs)

1. **Manifest in `slices/`, not in vault** — it's code metadata, must work in CI without Obsidian
2. **Keyed by `doc_id`, not path** — survives Obsidian file renames
3. **Obsidian for wiki, custom CLI for code bridge** — avoids building a wiki engine inside the slice tool
4. **Doc frontmatter decoupled from slice IDs** — single source of truth (DOCS.yaml), no dual-write drift
5. **Headless-first** — everything works without Obsidian running; it's optional visualization

## Current Prototype Status

Working prototype at `prototype/slices_cli.py`:
- ~1000 lines Python, argparse CLI
- Manifest loading/saving (`DOCS.yaml`)
- Drift detection (committed + uncommitted changes)
- `include`/`exclude` for sub-slice granularity
- All original slice commands preserved (list, show, files, deps, for, find, grep, check, sync-index)
- 40 tests passing

Not yet implemented:
- `affected-docs` command (designed, not coded)
- `doc_id` keying (manifest currently keyed by path)
- `docs-bootstrap` command (one-time migration)
- `vault_root` resolution
- Context engine integration code

## Open Questions

1. **Vault location**: Separate `wiki/` directory vs co-located with existing docs? Currently experimenting with a standalone `wiki/` dir.
2. **Bootstrap strategy**: Auto-generate DOCS.yaml from existing `tracks:` frontmatter, or build incrementally by hand?
3. **Staleness policy**: Warning-only, or eventually a CI gate? At what threshold?
4. **Cross-repo docs**: Some design knowledge spans repos. Personal Obsidian vault for those, or a shared one?
5. **Agent doc authoring**: Should agents create/update docs proactively, or only when flagged by staleness? What quality bar?
