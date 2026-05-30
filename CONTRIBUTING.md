# Contributing to slice-cli

Thanks for your interest. slice-cli is a small Rust CLI; contributions that keep
it simple and well-tested are very welcome.

## Dev setup

```bash
git clone https://github.com/scodge-24/slice-cli && cd slice-cli
cargo test --manifest-path rust/slice-rs/Cargo.toml       # suite builds temp git repos (~10s)
```

You need a Rust toolchain (pinned in `rust/slice-rs/rust-toolchain.toml`) and
`git` on PATH.

## Conventions

- **Keep changes focused.** The CLI lives in `rust/slice-rs/src/`; put code in the
  module that owns the concern.
- **Tests are not optional.** Every code path should have coverage; the suite uses
  temp git repos. Tests assert on baked-in expected values (`json!` literals for
  JSON output, inline strings for human/error output) — no external oracle.
- **Lints stay clean.** `cargo fmt --check` and `cargo clippy --all-targets -- -D
  warnings` run in CI.
- **Embedded templates stay in sync.** The `slice init` template constants must be
  byte-identical to `skills/slice-codebase/SKILL.md` and
  `agents/codebase-slicer.md` (guarded by `embedded_templates_match_committed_files`).
- **Staleness is fingerprint-anchored.** `slice stamp` records a content hash of
  a doc's tracked files; `verified_at` is a human note. See
  `docs/manifest-schema.md`.
- **`examples/mock-repo/` is sample data**, not docs about the tool.

## Before opening a PR

```bash
cargo fmt --check --manifest-path rust/slice-rs/Cargo.toml
cargo clippy --manifest-path rust/slice-rs/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path rust/slice-rs/Cargo.toml
```

Run these green, describe what changed and why, and link any related issue.

## Module map

| I want to change... | Look in (`rust/slice-rs/src/`) |
|---------------------|--------------------------------|
| repo / git / path discovery | `context.rs`, `git_backend.rs` |
| path normalization + globs | `paths.rs` |
| slice parsing + fingerprinting | `slices.rs`, `index.rs` |
| doc manifest load/save | `manifest.rs` |
| command handlers (list/show/stale/affected/stamp/…) | `commands.rs` |
| validation (`slice check`) + verification links | `check.rs` |
| `slice init` + embedded templates | `init.rs` |
| config (context ambiguity) | `config.rs` |
| CLI wiring / arg parsing | `cli.rs` |
| data types | `models.rs` |
| errors / exit codes | `error.rs`, `main.rs` |

## Filing issues

Open an issue with: what you ran, what you expected, what happened (include the
`slice ... ` command and any error output).
