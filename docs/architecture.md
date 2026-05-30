# Architecture: Slice CLI + Obsidian Doc Tracking

## Overview

Two independent systems collaborate through a shared manifest to solve documentation staleness in large codebases.

```
 Obsidian Vault (wiki/)                  Slice CLI (slices/)
 ┌──────────────────────┐               ┌──────────────────────┐
 │ Content authoring     │               │ Code slice ownership │
 │ [[wikilinks]]         │               │ files[], deps        │
 │ Tags, search, graph   │               │ Staleness detection  │
 │ doc_id in frontmatter │               │ Content fingerprints │
 └──────────┬────────────┘               └──────────┬───────────┘
            │                                       │
            └──────── DOCS.yaml (bridge) ───────────┘
                     doc_id → slice IDs
                     fingerprint (+ verified_at note)
```

**Obsidian** owns documentation content: prose, hierarchy via `[[wikilinks]]`, tags, search, human visualization. A vault is a directory of `.md` files with YAML frontmatter — fully functional without Obsidian running.

**Slice CLI** owns the code-to-doc bridge: which docs track which code slices, whether they're stale (by comparing a content fingerprint of each doc's tracked files against the fingerprint recorded at verification time; a git SHA-diff is the legacy fallback for entries with no fingerprint), and which docs are affected by a code change. The manifest lives in `slices/DOCS.yaml`.

**Neither system depends on the other.** Agents work headlessly with direct file reads and the slice CLI. Obsidian adds visualization and search for humans. The vault is portable plain markdown.

## Layer Boundaries

| Concern | Owner | Rationale |
|---|---|---|
| Doc content, prose, formatting | Vault (Obsidian) | Wiki tooling exists for this |
| Doc-to-doc relationships | Vault (`[[wikilinks]]`) | Obsidian graph view, backlinks |
| Doc categorization | Vault (frontmatter `tags`) | Obsidian search, Dataview |
| Doc-to-code mapping | Slice CLI (`DOCS.yaml`) | Requires slice awareness |
| Staleness detection | Slice CLI (`git diff`) | Requires git + slice files[] |
| "What docs are affected?" | Slice CLI (`affected-docs`) | Requires file→slice→doc resolution |
| "Find docs about X" | Vault (search, `rg`) | Content search, not code-aware |
| Stamping docs as reviewed | Slice CLI (`stamp`) | Updates manifest, not doc content |

## Key Invariants

1. **DOCS.yaml is the single source of truth** for doc-to-slice mapping. Docs do not reference slice IDs in their frontmatter. Slices do not reference docs in their frontmatter.

2. **doc_id is the stable key**, not file path. Obsidian users rename and move files. The manifest keys on `doc_id`, which is immutable in the doc's frontmatter.

3. **Headless mode requires only the slice CLI and filesystem access.** No Obsidian app, no REST API, no network. Agents read docs as plain files.

4. **Obsidian is optional for visualization.** The system works without it. Obsidian adds graph view, backlink navigation, and interactive search — but these are human ergonomics, not agent requirements.

5. **One vault per repo.** Design docs are tightly coupled to the code they describe. Cross-repo docs belong in a personal vault, not the project vault.

---

## ADR-001: Manifest in slices/, not in the vault

**Status**: Accepted

**Context**: The DOCS.yaml manifest maps docs to code slices and stores `verified_at` SHAs. It needs to be available in CI, headless agents, and any context where the slice CLI runs.

**Decision**: Store `DOCS.yaml` in `slices/`, alongside `INDEX.md` and slice definition files.

**Rationale**:
- The manifest is slice CLI metadata, not documentation content
- It must work in CI where Obsidian is absent
- Keeping it in `slices/` co-locates it with the slice definitions it references
- The vault stays portable — you can move or copy it without breaking the code bridge

**Consequences**:
- `DOCS.yaml` references vault paths relative to a `vault_root` field
- If the vault moves, only `vault_root` needs updating

---

## ADR-002: Key manifest by doc_id, not file path

**Status**: Accepted

**Context**: Obsidian users frequently rename and reorganize files. A manifest keyed by file path breaks on every rename.

**Decision**: Each doc has a `doc_id` field in its frontmatter. The manifest keys on `doc_id` and stores the current `path` as a separate field.

**Rationale**:
- `doc_id` is immutable and human-readable (e.g., `boundary-contract-spec`)
- File paths change when docs are reorganized in the vault
- The slice CLI resolves `doc_id` → `path` at query time
- A `slice check` validation pass can detect `doc_id` / `path` mismatches

**Consequences**:
- Every tracked doc must have `doc_id` in its frontmatter
- Bootstrap must generate `doc_id` values for existing docs
- Renames require updating `path` in DOCS.yaml (but not the key)

---

## ADR-003: Obsidian for wiki, not a custom implementation

**Status**: Accepted

**Context**: As we added doc dependencies, hierarchy, tags, CRUD, and search to the slice CLI, we recognized we were building a wiki engine. The slice CLI was growing beyond its core purpose.

**Decision**: Use Obsidian as the wiki layer. The slice CLI handles only the code-to-doc bridge.

**Rationale**:
- Obsidian handles hierarchy (`[[wikilinks]]`), tags, search, and graph visualization natively
- Vault format is plain `.md` files — no vendor lock-in, works headlessly
- Official CLI (v1.12+) provides JSON search and CRUD for interactive use
- Community tools (notesmd-cli, Local REST API, MCP servers) cover agent workflows
- Building these features into the slice CLI would duplicate existing, mature tooling

**Consequences**:
- Two tools instead of one — agents may need both `slice` and file reads
- Doc-to-doc relationships are expressed via `[[wikilinks]]`, not a manifest field
- The slice CLI does not need to understand doc content or hierarchy

---

## ADR-004: Doc frontmatter decoupled from slice IDs

**Status**: Accepted

**Context**: Earlier designs put `tracks:` (file path lists) or slice ID tags in doc frontmatter. This created dual-write drift between docs and the manifest.

**Decision**: Doc frontmatter contains only content-oriented metadata (`doc_id`, `title`, `kind`, `status`, `tags`, `summary`). No slice IDs, no file path lists.

**Rationale**:
- Single source of truth: DOCS.yaml owns the mapping, docs own the content
- No dual-write drift between two files
- Doc frontmatter stays small — avoids token bloat when agents read docs
- Tags in docs are human-semantic ("design", "numerics"), not operational ("rust-abc-types")

**Consequences**:
- Agents cannot determine a doc's tracked slices by reading the doc alone — they must query the manifest via `slice docs <id>` or `slice affected-docs`
- This is acceptable because agents should start from code context (slice CLI), not from doc context

---

## Component Interaction

### Agent reads code, needs design context

```
Agent changes src/auth/middleware.py
  → slice affected-docs src/auth/middleware.py --json
  → returns: [{doc_id: "auth-guide", path: "wiki/auth-guide.md", status: "stale", ...}]
  → Agent reads wiki/auth-guide.md
  → Agent updates doc if needed
  → slice stamp auth-guide
```

### Agent searches for design rationale

```
Agent needs to understand boundary contracts
  → rg "boundary" wiki/ (headless)
  → or: obsidian search query="boundary" (if Obsidian running)
  → reads wiki/boundary-contract-spec.md
```

### CI checks for stale docs

```
CI pipeline runs:
  → slice check --json
  → includes doc staleness warnings from DOCS.yaml
  → fails or warns based on policy
```

### Human browses doc relationships

```
Opens Obsidian → graph view shows [[wikilinks]] between docs
  → clicks through hierarchy
  → sees which docs are parents/children
  → slice stale-docs shows what needs review
```
