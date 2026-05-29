# slice-cli

CLI tool for navigating codebase slice documents with doc-staleness tracking. Turns `slices/*.md` into a query surface for humans and agents.

## Layout

- `slices_cli.py` — main CLI (the version we develop against)
- `slices_cli_upstream.py` — upstream reference (read-only, do not edit)
- `test_slices_cli.py` — pytest suite (tests create tmp git repos via fixtures)
- `examples/mock-repo/` — a self-contained demo repo the CLI runs against (`src/` mock code, `slices/` definitions + `DOCS.yaml`, `docs/` tracked docs). Run it with `slice --repo examples/mock-repo <cmd>`. This is sample data, NOT documentation about the tool.
- `design/` — design docs (architecture, schema, workflows, plans)
- `docs/` — reserved for real tool documentation (user-facing docs about slice-cli itself)

## Dev

- Python 3, depends on `pyyaml`
- Tests: `pytest test_slices_cli.py`
- The CLI is a single-file script; entry point is `main()` at the bottom of `slices_cli.py`

## Conventions

- `slices_cli_upstream.py` is the reference implementation — do not modify it. Compare against it when checking divergence.
- `DOCS.yaml` is the single source of truth for doc-to-slice mapping and staleness. Staleness is anchored on a content `fingerprint` of each doc's tracked files (recorded by `slice stamp`); `verified_at` is a human-readable HEAD note. Legacy entries without a fingerprint fall back to git SHA-diff. Docs and slice files stay clean of tracking metadata.
- Keep the CLI single-file. Avoid splitting into a package unless there's a strong reason.
