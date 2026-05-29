# slice-cli

CLI tool for navigating codebase slice documents with doc-staleness tracking. Turns `slices/*.md` into a query surface for humans and agents.

## Layout

- `slices_cli.py` — main CLI (the version we develop against)
- `slices_cli_upstream.py` — upstream reference (read-only, do not edit)
- `test_slices_cli.py` — pytest suite (tests create tmp git repos via fixtures)
- `src/` — mock codebase used by the slice documents and tests
- `slices/` — slice definitions (`INDEX.md`, per-slice `.md` files, `DOCS.yaml` manifest)
- `docs/` — documentation tracked for staleness via `DOCS.yaml`
- `design/` — design docs (architecture, schema, workflows)

## Dev

- Python 3, depends on `pyyaml`
- Tests: `pytest test_slices_cli.py`
- The CLI is a single-file script; entry point is `main()` at the bottom of `slices_cli.py`

## Conventions

- `slices_cli_upstream.py` is the reference implementation — do not modify it. Compare against it when checking divergence.
- `DOCS.yaml` is the single source of truth for doc-to-slice mapping and staleness (`verified_at` SHA). Docs and slice files stay clean of tracking metadata.
- Keep the CLI single-file. Avoid splitting into a package unless there's a strong reason.
