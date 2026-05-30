# Contributing to slice-cli

Thanks for your interest. slice-cli is a small Rust CLI with a Python parity
oracle; contributions that keep it simple and well-tested are very welcome.

## Dev setup

The shipped CLI is Rust (`rust/slice-rs`). Its parity tests shell out to the
Python oracle, so you need both toolchains:

```bash
git clone https://github.com/scodge-24/slice-cli && cd slice-cli
pip install -e ".[dev]"                                   # the parity oracle
cargo test --manifest-path rust/slice-rs/Cargo.toml       # Rust suite (~10s)
pytest                                                    # oracle suite
```

## Conventions

- **Keep changes focused.** The shipped CLI lives in `rust/slice-rs/src/`; put
  code in the module that owns the concern. The `slice_cli/` Python package is
  the parity oracle only — don't add features there (it's being phased out).
- **`slices_cli_upstream.py` is a read-only reference.** Do not edit it. Compare
  against it to understand divergence from the upstream implementation. It is not
  shipped.
- **Tests are not optional.** Every code path should have coverage; the suites
  use temp git repos. Prefer asserting on `--json` output over matching human
  text. New Rust behavior should be pinned by a parity test (against the Python
  oracle) or a committed snapshot.
- **Lints stay clean.** `cargo fmt --check` and `cargo clippy -- -D warnings` run
  in CI; the Python oracle stays `pyright`-clean.
- **Staleness is fingerprint-anchored.** `slice stamp` records a content hash of
  a doc's tracked files; `verified_at` is a human note. See
  `design/manifest-schema.md`.
- **`examples/mock-repo/` is sample data**, not docs about the tool.

## Before opening a PR

```bash
cargo fmt --check --manifest-path rust/slice-rs/Cargo.toml
cargo clippy --manifest-path rust/slice-rs/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path rust/slice-rs/Cargo.toml
pytest                                                    # oracle
```

Run these green, describe what changed and why, and link any related issue.

## Module map

The Rust CLI mirrors these concerns in `rust/slice-rs/src/` (`check.rs`,
`commands.rs`, `context.rs`, `paths.rs`, `index.rs`, `init.rs`, `slices.rs`,
`manifest.rs`, `git_backend.rs`, `cli.rs`). The table maps the Python oracle:

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
