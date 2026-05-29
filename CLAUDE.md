# slice-cli

CLI tool for navigating codebase slice documents with doc-staleness tracking. Turns `slices/*.md` into a query surface for humans and agents.

## Layout

- `slice_cli/` — main CLI package (the version we develop against)
- `slices_cli_upstream.py` — upstream reference (read-only, do not edit)
- pytest suite — tests create tmp git repos via fixtures
- `examples/mock-repo/` — a self-contained demo repo the CLI runs against (`src/` mock code, `slices/` definitions + `DOCS.yaml`, `docs/` tracked docs). Run it with `slice --repo examples/mock-repo <cmd>`. This is sample data, NOT documentation about the tool.
- `design/` — design docs (architecture, schema, workflows, plans)
- `docs/` — reserved for real tool documentation (user-facing docs about slice-cli itself)

## Dev

- Python 3, depends on `pyyaml`
- Tests: `pytest`
- The installed CLI entry point is `slice_cli.cli:main`; source-tree runs use
  `python -m slice_cli`.

## Conventions

- `slices_cli_upstream.py` is the reference implementation — do not modify it. Compare against it when checking divergence.
- `DOCS.yaml` is the single source of truth for doc-to-slice mapping and staleness. Staleness is anchored on a content `fingerprint` of each doc's tracked files (recorded by `slice stamp`); `verified_at` is a human-readable HEAD note. Legacy entries without a fingerprint fall back to git SHA-diff. Docs and slice files stay clean of tracking metadata.
- Keep changes focused in the module that owns the concern.

## Module map

| I want to change... | Look in |
|---------------------|---------|
| repo / git / path discovery | `slice_cli/context.py` |
| path normalization helpers | `slice_cli/paths.py` |
| content + source fingerprinting | `slice_cli/fingerprint.py` |
| index parse/generate | `slice_cli/index.py` |
| load/save slices + manifest | `slice_cli/persistence.py` |
| doc drift detection | `slice_cli/drift.py` |
| validation (`slice check`) | `slice_cli/check.py` |
| command handlers (`cmd_*`) | `slice_cli/commands.py` |
| human + JSON rendering | `slice_cli/render.py` |
| `slice docs bootstrap` | `slice_cli/bootstrap.py` |
| `slice init` + templates | `slice_cli/init.py` |
| argparse wiring + `main()` | `slice_cli/cli.py` |
| `python -m slice_cli` entry | `slice_cli/__main__.py` |
