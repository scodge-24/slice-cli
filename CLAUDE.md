# slice-cli

CLI tool for navigating codebase slice documents with doc-staleness tracking. Turns `slices/*.md` into a query surface for humans and agents.

## Layout

- `rust/slice-rs/` — the CLI (Rust). The `slice` binary builds from `src/main.rs`;
  logic lives in `src/`, one module per concern. Tests are native Rust
  (`tests/cli.rs` + module `#[cfg(test)]` units) and build throwaway git repos in
  tmp dirs.
- `examples/mock-repo/` — a self-contained demo repo the CLI runs against (`src/` mock code, `slices/` definitions + `DOCS.yaml`, `docs/` tracked docs). Run it with `slice --repo examples/mock-repo <cmd>`. This is sample data, NOT documentation about the tool.
- `docs/` — user-facing tool documentation: architecture, manifest schema, agent
  workflows, verification links.
- `design/` — local-only planning notes (gitignored, not published): the
  Python→Rust migration plans and scratch.

slice-cli was originally written in Python and ported to Rust; the Python
implementation is preserved at tag `python-impl-final` / branch `package-refactor`.

## Dev

- Rust (toolchain pinned in `rust/slice-rs/rust-toolchain.toml`); `git` on PATH
  at runtime.
- Build/run: `cargo run --manifest-path rust/slice-rs/Cargo.toml -- <args>`.
- Install: `cargo install --path rust/slice-rs` (produces `slice`).
- Tests: `cargo test --manifest-path rust/slice-rs/Cargo.toml`.
- Lint: `cargo fmt --check` + `cargo clippy --all-targets -- -D warnings`.

## Conventions

- `DOCS.yaml` is the single source of truth for doc-to-slice mapping and staleness. Staleness is anchored on a content `fingerprint` of each doc's tracked files (recorded by `slice stamp`); `verified_at` is a human-readable HEAD note. Legacy entries without a fingerprint fall back to git SHA-diff. Docs and slice files stay clean of tracking metadata.
- Keep changes focused in the module that owns the concern.
- The CLI writes only slice-owned state (`slices/DOCS.yaml` via `slice
  docs-bootstrap`, the `sync-index` artifact). Host/project policy — agent
  instructions, git hooks, CI workflows, `.claude/` files — is never written by the
  binary; it lives as agent-run setup guidance in `docs/setup.md`.

## Module map

| I want to change... | Look in (`rust/slice-rs/src/`) |
|---------------------|--------------------------------|
| repo / git / path discovery | `context.rs`, `git_backend.rs` |
| path normalization + globs | `paths.rs` |
| slice parsing + fingerprinting | `slices.rs`, `index.rs` |
| doc manifest load/save | `manifest.rs` |
| command handlers (list/show/stale/affected/stamp/…) | `commands.rs` |
| validation (`slice check`) + verification links | `check.rs` |
| config (context ambiguity) | `config.rs` |
| CLI wiring / arg parsing | `cli.rs` |
| data types | `models.rs` |
| errors / exit codes | `error.rs`, `main.rs` |
