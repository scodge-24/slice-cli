## Conventions

- `DOCS.yaml` is the single source of truth for doc-to-slice mapping and staleness. Staleness is anchored on a content `fingerprint` of each doc's tracked files (recorded by `slice stamp`); `verified_at` is a human-readable HEAD note. Legacy entries without a fingerprint fall back to git SHA-diff. Docs and slice files stay clean of tracking metadata.
- Keep changes focused in the module that owns the concern.
- The CLI writes only slice-owned state (`slices/DOCS.yaml` via `slice
  docs-bootstrap`, the `sync-index` artifact). Host/project policy — agent
  instructions, git hooks, CI workflows, `.claude/` files — is never written by the
  binary; it lives as agent-run setup guidance in `docs/setup.md`.
