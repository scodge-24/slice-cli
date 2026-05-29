# Contributing to slice-cli

Thanks for your interest. slice-cli is a small Python package; contributions
that keep it simple and well-tested are very welcome.

## Dev setup

```bash
git clone https://github.com/scodge-24/slice-cli && cd slice-cli
pip install -e ".[dev]"
pytest          # the suite builds throwaway git repos in tmp dirs (~10s)
```

## Conventions

- **Keep changes focused.** The CLI lives in the `slice_cli/` package; put code
  in the module that owns the concern instead of growing `cli.py`.
- **`slices_cli_upstream.py` is a read-only reference.** Do not edit it. Compare
  against it to understand divergence from the upstream implementation. It is not
  shipped in the published package.
- **Tests are not optional.** Every code path should have coverage; the suite
  uses temp git repos via the `repo` fixture. Prefer
  asserting on `--json` output over matching human text.
- **Types stay strict.** The code is fully type-hinted; `pyright` runs in CI.
- **Staleness is fingerprint-anchored.** `slice stamp` records a content hash of
  a doc's tracked files; `verified_at` is a human note. See
  `design/manifest-schema.md`.
- **`examples/mock-repo/` is sample data**, not docs about the tool.

## Before opening a PR

```bash
pytest
pyright    # if installed
```

Run these green, describe what changed and why, and link any related issue.

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

## Filing issues

Open an issue with: what you ran, what you expected, what happened (include the
`slice ... ` command and any error output).
