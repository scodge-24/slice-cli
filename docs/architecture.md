# Architecture: Slice CLI + Doc Staleness Tracking

## Overview

Two independent concerns collaborate through a shared manifest to solve documentation
staleness in large codebases: your **documentation** (any directory of Markdown files)
and the **Slice CLI** (code slice ownership + staleness).

```
 Your docs (docs/, wiki/, …)            Slice CLI (slices/)
 ┌──────────────────────┐               ┌──────────────────────┐
 │ Content authoring     │               │ Code slice ownership │
 │ Prose, links, headings│               │ files[], deps        │
 │ Whatever tooling you  │               │ Staleness detection  │
 │ like; doc_id in front │               │ Content fingerprints │
 └──────────┬────────────┘               └──────────┬───────────┘
            │                                       │
            └──────── DOCS.yaml (bridge) ───────────┘
                     doc_id → slice IDs
                     fingerprint (+ verified_at note)
```

**Your documentation** owns content: prose, hierarchy, links, tags — authored and
searched with whatever tools you prefer (plain Markdown, a static docs site, or a wiki
like Obsidian). slice-cli only needs a directory of `.md` files with a `doc_id` in each
file's YAML frontmatter. It is documentation-system-agnostic.

**Slice CLI** owns the code-to-doc bridge: which docs track which code slices, whether
they're stale (by comparing a content fingerprint of each doc's tracked files against the
fingerprint recorded at verification time; a git SHA-diff is the legacy fallback for
entries with no fingerprint), and which docs are affected by a code change. The manifest
lives in `slices/DOCS.yaml`.

**Neither side depends on a specific doc tool.** Agents and CI work headlessly with direct
file reads and the slice CLI. The docs are portable plain Markdown.

## Layer Boundaries

| Concern | Owner | Rationale |
|---|---|---|
| Doc content, prose, formatting | Your docs | Authoring is not slice-cli's job |
| Doc-to-doc relationships | Your docs (links) | Left to your Markdown / docs tooling |
| Doc categorization | Your docs (frontmatter `tags`) | Human-semantic, searchable via `slice find` |
| Doc-to-code mapping | Slice CLI (`DOCS.yaml`) | Requires slice awareness |
| Staleness detection | Slice CLI (fingerprint, git fallback) | Requires the slice `files[]` |
| "What docs are affected?" | Slice CLI (`affected-docs`) | Requires file→slice→doc resolution |
| "Find docs about X" | Your docs (`rg`, `slice find` on tags) | Content search, not code-aware |
| Stamping docs as reviewed | Slice CLI (`stamp`) | Updates manifest, not doc content |

## Key Invariants

1. **DOCS.yaml is the single source of truth** for doc-to-slice mapping. Docs do not
   reference slice IDs in their frontmatter. Slices do not reference docs in their
   frontmatter.

2. **doc_id is the stable key**, not file path. Docs get renamed and moved; the manifest
   keys on `doc_id`, which is immutable in the doc's frontmatter.

3. **Headless mode requires only the slice CLI and filesystem access.** No doc app, no REST
   API, no network. Agents read docs as plain files.

4. **Your doc tooling is optional.** The bridge works with nothing but a directory of
   Markdown. A wiki or docs site adds human ergonomics (graph view, search), not agent
   requirements.

5. **One docs root per repo.** Design docs are tightly coupled to the code they describe.
   Cross-repo docs belong in a personal knowledge base, not the project's `docs_root`.

---

## ADR-001: Manifest in slices/, not in the docs directory

**Status**: Accepted

**Context**: The DOCS.yaml manifest maps docs to code slices and records a staleness
anchor per doc. It needs to be available in CI, headless agents, and any context where the
slice CLI runs.

**Decision**: Store `DOCS.yaml` in `slices/`, alongside `INDEX.md` and slice definition
files.

**Rationale**:
- The manifest is slice CLI metadata, not documentation content
- It must work in CI and headless contexts independent of any doc tooling
- Keeping it in `slices/` co-locates it with the slice definitions it references
- The docs stay portable — you can move or copy them without breaking the code bridge

**Consequences**:
- `DOCS.yaml` references doc paths relative to a `docs_root` field
- If the docs directory moves, only `docs_root` needs updating

---

## ADR-002: Key manifest by doc_id, not file path

**Status**: Accepted

**Context**: Docs get renamed and reorganized. A manifest keyed by file path breaks on
every rename.

**Decision**: Each doc has a `doc_id` field in its frontmatter. The manifest keys on
`doc_id` and stores the current `path` as a separate field.

**Rationale**:
- `doc_id` is immutable and human-readable (e.g., `boundary-contract-spec`)
- File paths change when docs are reorganized
- The slice CLI resolves `doc_id` → `path` at query time
- A `slice check` validation pass can detect `doc_id` / `path` mismatches

**Consequences**:
- Every tracked doc must have `doc_id` in its frontmatter
- Bootstrap must generate `doc_id` values for existing docs
- Renames require updating `path` in DOCS.yaml (but not the key)

---

## ADR-003: Bring your own docs, don't bundle a wiki engine

**Status**: Accepted

**Context**: As we considered adding doc dependencies, hierarchy, tags, CRUD, and search
to the slice CLI, we recognized we were drifting toward building a wiki engine — well
beyond the tool's core purpose.

**Decision**: slice-cli does **not** own documentation content or its tooling. It tracks
the code-to-doc bridge over whatever Markdown docs you already have. Authoring, hierarchy,
and search are yours to choose (plain Markdown in `docs/`, a static site generator, or a
wiki like Obsidian if you want one).

**Rationale**:
- Markdown is universal — no vendor lock-in, works headlessly, every editor reads it
- Reimplementing hierarchy, search, and graph views would duplicate mature, existing tools
- Keeping slice-cli focused on the code bridge keeps it small and predictable

**Consequences**:
- Doc-to-doc relationships are expressed in your docs (e.g. Markdown links), not a manifest field
- The slice CLI does not need to understand doc content or hierarchy
- You can adopt or drop a doc tool without touching the code bridge

---

## ADR-004: Doc frontmatter decoupled from slice IDs

**Status**: Accepted

**Context**: Earlier designs put `tracks:` (file path lists) or slice ID tags in doc
frontmatter. This created dual-write drift between docs and the manifest.

**Decision**: Doc frontmatter contains only content-oriented metadata (`doc_id`, `title`,
`kind`, `status`, `tags`, `summary`). No slice IDs, no file path lists.

**Rationale**:
- Single source of truth: DOCS.yaml owns the mapping, docs own the content
- No dual-write drift between two files
- Doc frontmatter stays small — avoids token bloat when agents read docs
- Tags in docs are human-semantic ("design", "numerics"), not operational ("rust-abc-types")

**Consequences**:
- Agents cannot determine a doc's tracked slices by reading the doc alone — they must query
  the manifest via `slice docs <id>` or `slice affected-docs`
- This is acceptable because agents should start from code context (slice CLI), not doc context

---

## Component Interaction

### Agent reads code, needs design context

```
Agent changes src/auth/middleware.py
  → slice affected-docs src/auth/middleware.py --json
  → returns: [{doc_id: "auth-guide", path: "docs/auth-guide.md", status: "stale", ...}]
  → Agent reads docs/auth-guide.md
  → Agent updates doc if needed
  → slice stamp auth-guide
```

### Agent searches for design rationale

```
Agent needs to understand boundary contracts
  → slice find boundary            (locate the slice/concept in code)
  → rg "boundary" docs/            (plain content search over the docs)
  → reads docs/boundary-contract-spec.md
```

### CI checks for stale docs

```
CI pipeline runs:
  → slice check --json
  → includes doc staleness warnings from DOCS.yaml
  → fails or warns based on policy
```

### Human orients before editing

```
slice context src/auth/middleware.py   → owning slice, runtime flows, deps
slice deps auth-service --reverse --transitive  → blast radius
slice stale-docs                       → what needs review
```
