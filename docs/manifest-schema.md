# DOCS.yaml Manifest Schema

## Purpose

`slices/DOCS.yaml` is the bridge between documentation (any format — plain Markdown, a docs site, a wiki) and the slice CLI (code ownership). It maps each tracked doc to the code slices it describes and records when it was last verified as current. slice-cli is documentation-system-agnostic; it only needs a directory of Markdown files.

## Location

Always at `slices/DOCS.yaml`, alongside `INDEX.md` and slice definition files. This ensures the manifest is available in CI and headless environments without requiring access to the docs themselves.

## Schema

```yaml
# slices/DOCS.yaml

docs_root: ../docs               # path to the docs directory, relative to slices/
                                 # (the legacy key `vault_root:` is still accepted)

docs:
  boundary-contract-spec:        # doc_id (stable key, matches frontmatter)
    path: architecture/boundary-contract-spec.md   # relative to docs_root
    slices:                      # slice IDs this doc tracks
      - rust-abc-types
      - rust-abc-funcs-derivative
      - rust-abc-props
    fingerprint: 9f86d081...    # sha256 of tracked files at stamp time (staleness anchor)
    verified_at: abc123def456    # HEAD short-SHA at stamp time (human note)
    tags:                        # optional, human-semantic (mirrors doc frontmatter)
      - boundary
      - contracts
    include:                     # optional, narrows to specific files within slices
      - rust/crates/abc_types/src/state.rs
      - rust/crates/abc_types/src/fields.rs
    exclude:                     # optional, filters out noisy paths
      - "rust/crates/abc_types/src/tests/*"
```

## Field Reference

### Top-level

| Field | Type | Required | Description |
|---|---|---|---|
| `docs_root` | string | Yes | Path to the documentation directory, relative to the directory containing DOCS.yaml. The legacy key `vault_root` is still accepted on read (serde alias). |
| `docs` | map | Yes | Map of `doc_id` → doc entry |

### Per-doc entry

| Field | Type | Required | Description |
|---|---|---|---|
| `path` | string | Yes | Doc file path, relative to `docs_root` |
| `slices` | list[string] | Yes | Slice IDs this doc tracks. Must match existing `slice_id` values. |
| `fingerprint` | string | No | SHA-256 content hash of the doc's resolved tracked files at stamp time. The staleness anchor. Written by `slice stamp`. Absent on entries stamped before this field existed. |
| `verified_at` | string | No | HEAD short-SHA at stamp time. Human-readable note only — not used for staleness when `fingerprint` is present. Empty/`null` means "never verified". |
| `tags` | list[string] | No | Human-semantic tags. Searchable via `slice find`. |
| `include` | list[string] | No | If set, overrides slice-level `files[]` — only these paths are checked for drift. Supports globs. |
| `exclude` | list[string] | No | Paths/globs to exclude from drift detection. Applied after `include` or slice `files[]` resolution. |

## Drift Detection Logic

For each doc entry:

1. **Resolve tracked files**:
   - If `include` is set: use those paths directly
   - Otherwise: union all `files[]` from the doc's linked `slices`
   - Apply `exclude` filter (fnmatch glob matching), then expand globs to concrete files

2. **Decide staleness**:
   - **If `fingerprint` is present (preferred):** recompute the SHA-256 content hash
     of the resolved files. Stale iff it differs from the recorded `fingerprint`.
     This is independent of git history, so it is correct across the
     edit→stamp→commit ordering and survives rebases/amends.
   - **If `fingerprint` is absent (legacy fallback):** use git diff against
     `verified_at` — `git diff --name-only <verified_at>..HEAD -- <files>` unioned
     with `git diff --name-only HEAD -- <files>`. Stale if non-empty. Empty/null
     `verified_at` → unverified (always stale); invalid SHA → git error, treat as stale.

Re-stamping a legacy entry migrates it to a fingerprint.

## Doc Frontmatter Schema

Tracked docs use content-oriented frontmatter. Slice IDs do not appear here. The one
optional exception is `tracks:` — a bootstrap-only list of code paths (see below).

```yaml
---
doc_id: boundary-contract-spec      # required, stable, matches manifest key
title: Boundary Contract Specification
kind: design                        # design | guide | reference | adr | runbook
status: active                      # active | draft | archived | superseded
tags: [boundary, contracts, ownership]
aliases: [BCS, boundary spec]       # optional alternate names
tracks:                             # optional, bootstrap-only seed (see field reference)
  - rust/crates/abc_types/src/state.rs
summary: >-
  Ownership rules, naming invariants, and adapter patterns
  for the Rust simulation pipeline.
---
```

### Frontmatter fields

| Field | Required | Description |
|---|---|---|
| `doc_id` | Yes | Immutable identifier. Must match the key in DOCS.yaml. |
| `title` | Yes | Human-readable title. |
| `kind` | No | Document type. Useful for filtering. |
| `status` | No | Lifecycle state. `archived` docs may be excluded from staleness checks. |
| `tags` | No | Human-semantic tags. Searchable via `slice find`. |
| `aliases` | No | Optional alternate names for the doc. |
| `summary` | No | One-sentence description for agent context. Low token cost, high signal. |
| `tracks` | No | Bootstrap-only list of code paths the doc describes. Read once by `slice init --docs` / `slice docs-bootstrap` to resolve the doc's owning slices into `DOCS.yaml`, then ignored — not stored in the manifest and not a runtime mapping source. |

## Validation Rules (slice check)

The `slice check` command validates the manifest:

| Check | Severity | Description |
|---|---|---|
| Doc file exists | Error | `docs_root` + `path` must resolve to a real file |
| Slice IDs exist | Error | Every entry in `slices` must match a known `slice_id` |
| doc_id matches frontmatter | Error | The manifest key must match `doc_id` in the doc's YAML frontmatter |
| verified_at is valid SHA | Warning | If set, should resolve via `git rev-parse` |
| No duplicate doc_ids | Error | Each doc_id appears at most once |
| No orphan docs | Warning | Docs with `doc_id` frontmatter that aren't in the manifest |

## Examples

### Single-slice doc

```yaml
docs:
  auth-guide:
    path: guides/auth-guide.md
    slices: [auth-service]
    verified_at: 57e4d1a4caf7
    tags: [auth, security]
```

### Multi-slice doc (design spec spanning subsystems)

```yaml
docs:
  boundary-contract-spec:
    path: architecture/boundary-contract-spec.md
    slices:
      - rust-abc-types
      - rust-abc-funcs-derivative
      - rust-abc-props
      - rust-abc-runner-mcd-replay
    verified_at: afcfa3b01234
    tags: [boundary, contracts]
```

### Doc with sub-slice granularity

```yaml
docs:
  replay-trace-refactor:
    path: architecture/replay-trace-refactor-plan.md
    slices: [rust-abc-types, rust-abc-funcs-derivative]
    verified_at: e9bc69c
    include:
      - rust/crates/abc_types/src/pack.rs
      - rust/crates/abc_types/src/call_trace.rs
      - rust/crates/abc_funcs/src/_528095a7339f/types.rs
```

### Newly added, unverified doc

```yaml
docs:
  proptest-harness-modes:
    path: specs/proptest-harness-modes-spec.md
    slices: [rust-abc-funcs-leaf, mcd-harness]
    verified_at: null
    tags: [proptest, parity]
```
