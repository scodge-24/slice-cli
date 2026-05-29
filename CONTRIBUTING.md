# Contributing to slice-cli

Thanks for your interest. slice-cli is a single-file Python CLI; contributions
that keep it simple and well-tested are very welcome.

## Dev setup

```bash
git clone https://github.com/scodge-24/slice-cli && cd slice-cli
pip install -e ".[dev]"
pytest          # the suite builds throwaway git repos in tmp dirs (~10s)
```

## Conventions

- **Keep it single-file.** The CLI lives in `slices_cli.py`. Don't split it into
  a package without a strong reason.
- **`slices_cli_upstream.py` is a read-only reference.** Do not edit it. Compare
  against it to understand divergence from the upstream implementation. It is not
  shipped in the published package.
- **Tests are not optional.** Every code path should have coverage; the suite
  uses temp git repos via the `repo` fixture (see `test_slices_cli.py`). Prefer
  asserting on `--json` output over matching human text.
- **Types stay strict.** The code is fully type-hinted; `pyright` runs in CI.
- **Staleness is fingerprint-anchored.** `slice stamp` records a content hash of
  a doc's tracked files; `verified_at` is a human note. See
  `design/manifest-schema.md`.
- **`examples/mock-repo/` is sample data**, not docs about the tool.

## Before opening a PR

```bash
pytest
pyright slices_cli.py    # if installed
```

Run these green, describe what changed and why, and link any related issue.

## Filing issues

Open an issue with: what you ran, what you expected, what happened (include the
`slice ... ` command and any error output).
