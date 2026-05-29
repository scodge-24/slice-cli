---
doc_id: rust-prototype-plan
title: Rust Prototype Plan
tags: [plan, rust, cli, performance]
---

# Rust Prototype Plan

Status: draft
Generated: 2026-05-29

## Summary

Build a Rust prototype of slice-cli to prove whether a native binary materially
improves agent-loop latency while preserving the Python tool's behavior. The
prototype should coexist with the Python package until parity, performance, and
maintenance costs are proven.

The Python baseline in `bench/python-baseline.md` changes the port rationale:
the fixed cost is mostly Python interpreter startup and imports, not slice
processing. Real command timings are about 240-330 ms per invocation on the
meals repo, while a throwaway Rust `list --json` spike ran in 3.33 ms. Git
subprocess calls are much smaller, about 4-7 ms each, so replacing git matters
only for commands that otherwise qualify for the sub-5 ms target.

## Goals

- Produce a Rust binary that can run the common read-only agent commands in
  under 5 ms on warm-cache real repos.
- Preserve command behavior, JSON shapes, and exit-code semantics for the
  commands included in the prototype.
- Avoid git subprocesses on hot paths that do not semantically require git.
- Evaluate native git access behind an abstraction instead of committing to
  gitoxide before measurement.
- Keep the Python implementation as the correctness oracle during the prototype.

## Non-Goals

- Do not replace the Python package in the first Rust PR.
- Do not port `slice init`, embedded agent templates, or `docs-bootstrap` until
  read-only navigation and staleness are proven.
- Do not hand-roll a full git implementation. Native git access must come from a
  maintained library or stay as subprocess fallback.
- Do not chase sub-5 ms for commands whose semantics require whole-repo status
  or diff work unless native git proves it feasible.

## Baseline Targets

Use `bench/python-baseline.md` as the starting comparison point.

| Path | Existing Python | Rust target |
|------|-----------------|-------------|
| `list --json` on meals | 271.86 ms avg | <5 ms |
| `context <file>` on meals | 240-290 ms | <5 ms for ownership + slice context |
| `affected-docs <file>` on meals | 240-270 ms | <5 ms when given explicit paths |
| `check` on meals | 280-330 ms | no initial sub-5 ms target |
| `stale-docs` on meals | 250-290 ms | <5 ms for fingerprint-only manifests; measured separately for legacy git fallback |

The first benchmark gate should use release builds, warm filesystem cache, and
the same machine/repo setup as the Python baseline.

## Architecture

Place the prototype under `rust/slice-rs/`:

```text
rust/slice-rs/
  Cargo.toml
  rust-toolchain.toml
  src/
    main.rs
    lib.rs
    cli.rs
    context.rs
    models.rs
    manifest.rs
    slices.rs
    paths.rs
    fingerprint.rs
    git.rs
    commands/
      mod.rs
      list.rs
      show.rs
      affected_docs.rs
      stale_docs.rs
  tests/
    parity.rs
  benches/
    hot_paths.rs
```

Rust conventions:

- Edition 2024.
- `#![forbid(unsafe_code)]`.
- `clap` derive for CLI parsing.
- `serde`, `serde_json`, and `serde_yml` for data formats.
- `thiserror` for library errors and `anyhow` only at the binary boundary.
- `rustc-hash` for internal maps where hashing shows up in profiles.
- `criterion` or a small custom harness for wall-clock hot-path checks.

TheAlgorithms/Rust is useful as a reference for lint posture and simple module
fanout, not as a CLI architecture template. Lift the idea of broad clippy groups
with explicit local allows, and consider property-style tests for deterministic
path/fingerprint behavior. Do not copy its full lint allow list.

## Git Strategy

Use a narrow `GitBackend` trait so command logic does not care how git data is
retrieved:

```rust
trait GitBackend {
    fn repo_root(&self) -> Result<PathBuf>;
    fn head_sha(&self) -> Result<Option<String>>;
    fn changed_files_since(&self, base: &str, paths: &[RepoPath]) -> Result<Vec<RepoPath>>;
    fn worktree_changed_files(&self, paths: &[RepoPath]) -> Result<Vec<RepoPath>>;
}
```

Implement backends in this order:

1. `NoGitFastPath`
   - Parent-walk for `.git` and repo-relative path formatting.
   - Used by `list`, `show`, `files`, `deps`, `for`, `find`, and
     `affected-docs <explicit paths>`.
   - This is the main source of sub-5 ms behavior.

2. `ProcessGitBackend`
   - Correctness baseline using `git` subprocesses.
   - Acceptable for `stamp`, legacy SHA fallback, and commands where 4-7 ms git
     calls are not the dominant product cost.

3. Native git backend
   - Prefer `gix` if its APIs cover the needed repo discovery, HEAD, status, and
     diff operations with less overhead than subprocess git.
   - Keep `git2` as a comparison candidate if `gix` integration cost is too high.
   - Choose based on benchmarked command latency and implementation complexity,
     not preference.

Important: fingerprint-based staleness does not need git to decide whether a doc
is current. Git is needed for human `verified_at`, legacy SHA fallback, and
changed-file explanations after a fingerprint mismatch. The Rust prototype
should avoid git work until a command actually needs one of those details.

## Implementation Phases

### Phase 1 - Scaffold + Read-Only Navigation

Commands:

- `list`
- `show`
- `files`
- `deps`
- `for`

Requirements:

- Parse `slices/*.md` frontmatter and body.
- Preserve existing JSON shapes for implemented commands.
- Do not touch git for these commands.
- Add parity tests against Python output for `examples/mock-repo`.

Verify:

```bash
cargo test --manifest-path rust/slice-rs/Cargo.toml
cargo run --release --manifest-path rust/slice-rs/Cargo.toml -- --repo examples/mock-repo list --json
```

### Phase 2 - Manifest + Affected Docs

Commands:

- `affected-docs`
- `docs`
- `stale-docs` for fingerprinted manifests

Requirements:

- Parse `slices/DOCS.yaml`.
- Implement doc-to-slice reverse lookup.
- Implement explicit-path affected-doc lookup without git.
- Implement content fingerprint comparison for manifest entries that already
  have `fingerprint`.

Verify:

- JSON parity for `affected-docs`, `docs`, and clean fingerprinted `stale-docs`.
- Benchmark `affected-docs <path> --json` against meals.

### Phase 3 - Git-Backed Staleness

Commands:

- `stale-docs` with changed-file details.
- `stamp`.
- Legacy SHA-diff fallback.

Requirements:

- Start with `ProcessGitBackend` for parity.
- Add native backend only after the subprocess baseline is measured.
- Preserve dirty-tree stamp behavior from Python: stamping records current file
  contents, not "last committed" contents.

Verify:

- Python parity tests for dirty tree, edit-stamp-commit, and rebase-after-stamp
  scenarios.
- Benchmark git-backed commands with both subprocess and native backends.

### Phase 4 - Validation

Commands:

- `check`
- `sync-index`

Requirements:

- Match Python validation errors and warning categories.
- Preserve source fingerprint semantics for `INDEX.md`.
- Keep hidden index drift warning behavior.

Verify:

- Port the Python check tests to Rust parity tests.
- Add snapshot tests for `check --json`.

## Testing

Testing must prove behavior parity before performance claims matter.

- Unit tests:
  - path normalization and glob/literal metachar handling
  - frontmatter parsing
  - manifest parsing and serialization
  - content fingerprint determinism
  - dependency traversal and cycle handling

- Parity integration tests:
  - invoke Python `slice_cli.cli:main` or the installed Python command as oracle
  - invoke Rust `slice-rs`
  - compare JSON outputs exactly for stable commands
  - compare exit codes for success, stale-status, and user-error cases

- Fixture tests:
  - use `examples/mock-repo`
  - create temporary git repos matching the current Python pytest fixture
  - cover dirty-tree fingerprint behavior, legacy SHA fallback, and missing docs

- Property tests:
  - fingerprint is order-independent
  - path normalization is idempotent
  - equivalent include/exclude sets produce equivalent tracked-file sets

- Snapshot tests:
  - `check --json`
  - `stale-docs --json`
  - representative human output only where the text is intentionally stable

## CI

Add a Rust CI job once the scaffold lands:

```yaml
rust:
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v4
    - uses: dtolnay/rust-toolchain@master
      with:
        toolchain: "stable"
        components: rustfmt, clippy
    - uses: Swatinem/rust-cache@v2
      with:
        workspaces: rust/slice-rs
    - run: cargo fmt --check --manifest-path rust/slice-rs/Cargo.toml
    - run: cargo clippy --all-targets --manifest-path rust/slice-rs/Cargo.toml -- -D warnings
    - run: cargo test --manifest-path rust/slice-rs/Cargo.toml
    - run: cargo build --release --manifest-path rust/slice-rs/Cargo.toml
    - run: rust/slice-rs/target/release/slice-rs --repo examples/mock-repo list --json
```

Prefer path filtering so the Rust job runs on changes to:

- `rust/**`
- `examples/mock-repo/**`
- `slices/**` if this repo later gains first-party slices
- `.github/workflows/**`

Keep the Python CI job active. The Rust prototype is additive until replacement
is explicitly accepted.

## Performance Monitoring

Add a benchmark file next to the prototype:

```text
bench/
  python-baseline.md
  rust-baseline.md
  compare-rust-python.sh
```

The comparison script should:

- build Rust in release mode
- run each command at least 30 times with warm cache
- record mean, min, max, and p95
- run Python and Rust against the same repo and command set
- write markdown output so results can be reviewed in PRs

Minimum command set:

```bash
slice-rs --repo examples/mock-repo list --json
slice-rs --repo examples/mock-repo show auth-service --json
slice-rs --repo examples/mock-repo for src/auth/middleware.py --json
slice-rs --repo examples/mock-repo affected-docs src/auth/middleware.py --json
slice-rs --repo examples/mock-repo stale-docs --json
```

Real-repo benchmark set should include the meals repo when available locally:

```bash
slice-rs --repo /home/scodge/dev/meals list --json
slice-rs --repo /home/scodge/dev/meals context <representative-file> --json
slice-rs --repo /home/scodge/dev/meals affected-docs <representative-file> --json
slice-rs --repo /home/scodge/dev/meals stale-docs --json
slice-rs --repo /home/scodge/dev/meals check --json
```

CI should not fail on absolute timing at first because hosted runners are noisy.
Instead:

- always run functional parity tests in CI
- run benchmark smoke in CI and upload/print results
- enforce local acceptance thresholds before promoting Rust as replacement
- optionally add a `PERF_STRICT=1` mode for dedicated machines

Track regressions by updating `bench/rust-baseline.md` after meaningful changes.
Every benchmark update must state hardware, OS, Rust toolchain, repo sizes, and
whether caches were warm.

## Acceptance Criteria

The Rust prototype is accepted as successful when all of these are true:

- Source lives under `rust/slice-rs/` and builds with `cargo build --release`.
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and
  `cargo test` pass.
- Implemented commands preserve Python JSON shapes and exit codes for:
  - `list --json`
  - `show <slice> --json`
  - `files <slice> --json`
  - `deps <slice> --json`
  - `for <path> --json`
  - `affected-docs <path> --json`
  - `stale-docs --json` for fingerprinted manifests
- Python parity tests cover at least `examples/mock-repo` and one temporary
  dirty-git fixture.
- Release binary, warm-cache local benchmark hits:
  - `list --json` on meals: <5 ms average over 30 runs
  - `for <path> --json` on meals: <5 ms average over 30 runs
  - `affected-docs <path> --json` on meals with explicit changed path: <5 ms
    average over 30 runs
- Any command that shells out to git reports the subprocess cost separately from
  Rust parsing/rendering cost.
- Native git backend decision is documented with benchmark evidence:
  - keep subprocess git where it is simpler and fast enough
  - use `gix` or `git2` only where it removes measurable latency or improves
    distribution ergonomics
- CI includes Rust format, lint, test, release build, and mock-repo smoke checks.
- `bench/rust-baseline.md` exists and compares Rust against
  `bench/python-baseline.md`.

## Replacement Gate

Replacing the Python CLI is a separate decision. Do not replace it until:

- all user-facing commands are ported or intentionally dropped
- docs and install paths are updated
- wheel/pipx/plugin behavior has a Rust-compatible distribution story
- Python and Rust outputs match on the full current test suite
- the maintenance burden is lower than the latency saved in real agent loops
