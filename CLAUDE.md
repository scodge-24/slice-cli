# slice-cli

CLI tool for navigating codebase slice documents with doc-staleness tracking. Turns `slices/*.md` into a query surface for humans and agents.

## Layout

- `rust/slice-rs/` — **the shipped CLI** (Rust). The `slice` binary builds from
  `src/main.rs`; logic lives in `src/` (one module per concern, mirroring the
  Python oracle). This is the version we develop against going forward.
- `slice_cli/` — Python implementation, now the **parity test oracle** only
  (`rust/slice-rs/tests/parity.rs` shells out to `python -m slice_cli`). Being
  phased out per `design/rust-gap-closure-plan.md`; do not add features here.
- `slices_cli_upstream.py` — upstream reference (read-only, do not edit)
- pytest suite (`test_slices_cli.py`) — the oracle's test net; the source of the
  expected behavior the Rust suite must match
- `examples/mock-repo/` — a self-contained demo repo the CLI runs against (`src/` mock code, `slices/` definitions + `DOCS.yaml`, `docs/` tracked docs). Run it with `slice --repo examples/mock-repo <cmd>`. This is sample data, NOT documentation about the tool.
- `design/` — design docs (architecture, schema, workflows, plans)
- `docs/` — reserved for real tool documentation (user-facing docs about slice-cli itself)

## Dev

- Rust (toolchain pinned in `rust/slice-rs/rust-toolchain.toml`); `git` on PATH
  at runtime.
- Build/run: `cargo run --manifest-path rust/slice-rs/Cargo.toml -- <args>`.
  Install: `cargo install --path rust/slice-rs` (produces `slice`).
- Tests: `cargo test --manifest-path rust/slice-rs/Cargo.toml` (parity tests need
  the Python oracle: `pip install -e ".[dev]"` so `python -m slice_cli` resolves).
- Oracle suite: `pytest`.

## Conventions

- `slices_cli_upstream.py` is the reference implementation — do not modify it. Compare against it when checking divergence.
- `DOCS.yaml` is the single source of truth for doc-to-slice mapping and staleness. Staleness is anchored on a content `fingerprint` of each doc's tracked files (recorded by `slice stamp`); `verified_at` is a human-readable HEAD note. Legacy entries without a fingerprint fall back to git SHA-diff. Docs and slice files stay clean of tracking metadata.
- Keep changes focused in the module that owns the concern.

## Module map

The Rust CLI mirrors these concerns one-to-one in `rust/slice-rs/src/`
(`check.rs`, `commands.rs`, `context.rs`, `paths.rs`, `index.rs`, `init.rs`,
`slices.rs`, `manifest.rs`, `git_backend.rs`, `cli.rs`). The table below maps the
Python oracle:

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
