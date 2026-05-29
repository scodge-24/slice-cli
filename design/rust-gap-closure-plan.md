---
doc_id: rust-gap-closure-plan
title: Rust Gap Closure Plan
tags: [plan, rust, cli, parity]
---

# Rust Gap Closure Plan

Status: implemented; replacement deferred
Generated: 2026-05-29
Last updated: 2026-05-29

## Summary

The Rust prototype proves the Python-baseline hypothesis: read-only hot-path
commands can run in a few milliseconds by avoiding Python startup and avoiding
git where command semantics do not need it. It is not yet a replacement for the
Python CLI. This plan closes the feature, parity, test, and distribution gaps in
small lanes while preserving the prototype's performance discipline.

Principles:

- Python remains the behavior oracle until Rust parity is complete.
- Preserve JSON shapes and exit codes before optimizing deeper.
- Keep no-git hot paths no-git.
- Add native git only where measured subprocess git cost matters.
- Do not switch the public `slice` entry point until replacement criteria pass.

## Current Rust Coverage

Implemented:

- `list`
- `show` basic metadata, body, and standard section flags
- `files`
- `deps` with direct, reverse, and transitive modes
- `for`
- `find`
- `grep`
- `docs`
- `context` JSON for slice/file resolution and standard sections
- `affected-docs` for explicit paths
- `stale-docs` with process-git attribution baseline
- native `sync-index`, `stamp`, and `docs-bootstrap`
- native `check --json` and human summary output
- native `init`

Implemented test/monitoring support:

- Rust unit tests for path and frontmatter basics.
- Rust integration parity tests against Python for stable JSON hot paths.
- Rust CI job for fmt, clippy, tests, release build, and smoke.
- `bench/compare-rust-python.sh`.
- `bench/rust-baseline.md`.

Progress:

- Lane 1 read-only parity now has native Rust support for `find`, `docs`,
  `show --body`, `show --system`, `show --call-stacks`, `show --verification`,
  and parity coverage for JSON outputs on `examples/mock-repo`.
- Lane 2 now loads `slices/config.yaml` for context ambiguity, supports
  `slice-rs context --strict` and `--best-effort`, preserves manifest order, and
  has unit coverage for glob expansion and literal filenames containing
  metacharacters.
- Lane 3 now has a process-git attribution baseline for `stale-docs`, including
  a temp-repo parity test proving fingerprint drift narrows changed files.
- Lane 4 now has native Rust ports for `stamp`, `sync-index`, and
  `docs-bootstrap`, including temp-repo tests for stdout, write mode, dry-run,
  bad target exits, fingerprint stamping, and manifest loading.
- Lane 5 now has a native Rust `check` baseline covering structural validation,
  index consistency/staleness, staged coverage, DOCS.yaml validation, doc drift,
  and verification links, with JSON parity tests for mock, context/config, index,
  staged-coverage, doc-drift, manifest-error, and verification-link fixtures.
- Lane 6 now has native `init`, with embedded skill/agent templates sourced from
  the committed plugin files and tests for dry-run, hook/CI writes, idempotent
  agent blocks, and loose agent installs.
- Lane 7 replacement decision is explicit: keep Python as the supported public
  `slice` entry point for now, and keep Rust as a source-checkout prototype
  binary until a release/distribution design is approved.

## Remaining Decisions

Missing native Python-command ports: none.

Replacement is deferred, not accepted:

- `grep` is native but remains subprocess-based by design because search is
  inherently delegated to `rg`.
- Git attribution uses a `GitBackend` trait with `ProcessGitBackend` as the
  correctness baseline. A native `gix` backend is not adopted in this pass
  because measured hot paths avoid git entirely and remain under 5 ms; the git
  subprocess is paid only by attribution/status commands where correctness
  matters more than a speculative dependency.
- The Rust prototype is documented as source-checkout-only. Public installation
  still uses the Python `slice` entry point until packaging, plugin fallback,
  and release ownership are approved.

## Lane 1 - Complete Read-Only Parity

Goal: finish commands that can remain no-git or shell out only to existing tools.

Commands/features:

- `find`
- `docs`
- `show --body`
- `show --system`
- `show --call-stacks`
- `show --verification`
- full human output parity for `list`, `show`, `files`, `deps`, `for`, `docs`

Implementation notes:

- Reuse existing loaded slice and manifest data.
- Keep `find` no-git; search slice id, description, files, abstractions,
  dependencies, manifest tags, and body just like Python.
- Keep `docs` no-git unless stale/current status is requested; then use the same
  staleness helper as `stale-docs`.
- Extend section parsing once and share it between `show` and `context`.

Testing:

- Add parity tests for JSON outputs.
- Add focused snapshot tests for human output where text is intentionally
  stable.
- Add tests for missing sections and section flag combinations.

Acceptance:

- All Lane 1 JSON outputs exactly match Python on `examples/mock-repo`.
- `cargo test`, clippy, and Python tests pass.
- Existing hot-path benchmark means stay under 5 ms on `examples/mock-repo`.

## Lane 2 - Context Config + Path Semantics

Goal: close context and path edge cases before implementing write commands.

Features:

- `slices/config.yaml` parsing.
- `context.ambiguity: strict | best_effort`.
- `slice-rs context --strict`.
- `slice-rs context --best-effort`.
- Python-equivalent path normalization for:
  - absolute paths
  - `./` prefixes
  - literal filenames containing glob metacharacters
  - simple globs in slice `files`
  - include/exclude filtering

Implementation notes:

- If full glob semantics become non-trivial, use a maintained crate and measure
  its startup/runtime cost.
- Keep a small path module with unit tests; do not distribute path logic across
  commands.

Testing:

- Port Python context config tests.
- Add property tests for path normalization idempotence.
- Add regression tests for `app/[id]/page.tsx` literal paths.

Acceptance:

- Context behavior and exit codes match Python for strict, best-effort, missing
  owner, and ambiguous-owner cases.
- Path/glob tests cover current Python regressions.

## Lane 3 - Git Backend + Staleness Parity

Goal: make `stale-docs`, `affected-docs`, and future `stamp` semantically
correct for fingerprinted and legacy manifests.

Features:

- `GitBackend` trait.
- `ProcessGitBackend` correctness baseline.
- native backend decision:
  - keep `ProcessGitBackend` for now
  - reconsider `gix` only if attributed git commands become a measured user
    bottleneck
  - avoid `git2` unless `gix` is too complex or slower in practice
- real changed-file attribution for stale fingerprinted docs.
- legacy SHA-diff fallback parity.
- dirty-worktree detection parity.

Implementation notes:

- Fingerprint equality should remain the primary stale/current gate when a
  manifest entry has `fingerprint`.
- Git is only needed for changed-file explanation, legacy SHA fallback, and
  `verified_at`.
- Keep subprocess git if native backend is not measurably better or adds too
  much complexity.

Testing:

- Port Python tests for:
  - drift after source change
  - dirty worktree drift
  - include/exclude narrowing
  - bad SHA reports
  - fingerprint drift narrows changed files
  - edit-stamp-commit not stale
  - rebase-after-stamp not stale
- Run the same tests under process backend and any native backend.

Acceptance:

- Rust `stale-docs --json` matches Python across clean, dirty, fingerprinted,
  and legacy fixtures.
- Backend benchmark is documented in `bench/rust-baseline.md` or a successor
  file.

## Lane 4 - Write Commands

Goal: port state-changing commands with safe, test-backed behavior.

Commands:

- `stamp`
- `sync-index`
- `docs-bootstrap`

Implementation notes:

- `stamp` must preserve Python's dirty-tree semantics: record current file
  contents and write current HEAD as a human note.
- `sync-index` must preserve existing row order, append new slices
  alphabetically, and write the source fingerprint.
- `docs-bootstrap` must preserve symlink-relative vault behavior and
  `--dry-run` output.

Testing:

- Port Python stamp tests.
- Add temp-repo tests for `sync-index --check`, `--stdout`, and write mode.
- Add bootstrap dry-run and force-overwrite tests.

Acceptance:

- Rust write commands produce byte-compatible YAML/index output where Python
  currently does.
- All write commands support dry-run/check modes where Python does.

## Lane 5 - Validation Parity

Goal: port `check` without weakening its signal.

Features:

- Structural slice validation.
- file existence and glob validation.
- dependency resolution.
- overlap detection.
- `INDEX.md` consistency and staleness.
- staged source coverage.
- doc manifest validation.
- V-model verification link checks.
- `--strict-index`
- `--no-staleness`
- `--no-staged-coverage`
- `--no-doc-drift`
- `--require-verification`
- `--json`

Testing:

- Port the full Python `TestCheck` and `TestVerificationLinks` coverage.
- Snapshot `check --json` for clean and failing fixtures.
- Assert warning categories and hidden-warning counts match Python.

Acceptance:

- `slice-rs check --json` matches Python for all current check fixtures.
- Human output is close enough for users; exact text parity only where tests
  depend on it.

## Lane 6 - Init, Grep, and Distribution

Goal: cover the remaining commands and decide whether Rust can replace Python.

Commands/features:

- `grep`
- `init`
- install/package story
- plugin fallback story
- README and changelog updates

Implementation notes:

- `grep` can continue invoking `rg`; this command is inherently subprocess
  based.
- `init` ports embedded templates using the committed plugin skill/agent files
  as the source of truth.
- Distribution is a separate product decision:
  - keep Python package and ship `slice-rs` as optional companion
  - use a Python wheel that bundles the Rust binary
  - switch releases to a native binary distribution

Testing:

- `grep` missing-rg behavior.
- `init --dry-run`, `--agent`, `--hook`, `--ci`, `--global`.
- embedded template sync tests if templates move to Rust.

Acceptance:

- Every Python subcommand has a Rust equivalent or a documented intentional
  non-port.
- Installation docs describe how users get both `slice` and `slice-rs`.

## Lane 7 - Replacement Decision

Goal: decide whether Rust should become the primary implementation.

Replacement requires:

- command surface parity or documented removals
- full Python test-suite parity
- benchmark evidence on mock and meals repos
- CI green for Python and Rust
- a clear release/distribution path
- no performance regression on existing Rust hot paths
- no unbounded maintenance burden from duplicated implementations

Decision:

- Not accepted as the primary public implementation in this pass.
- Keep Python packaging and the `slice` console script as the supported install
  path.
- Keep `slice-rs` available from a source checkout for parity/performance
  testing.
- Do not bundle the Rust binary into the wheel or plugin until release ownership
  is decided.

If later accepted:

- switch public docs from prototype language to primary language
- decide whether `slice` invokes Rust directly
- keep Python as compatibility layer only if needed
- update plugin fallback instructions
- update changelog with breaking/non-breaking migration details

## Tracking Checklist

- [x] Lane 1 - read-only JSON command parity
- [x] Lane 1 - full human-output parity
- [x] Lane 2 - context config and path semantics baseline
- [x] Lane 2 - full Python fixture coverage
- [x] Lane 3 - process-git staleness attribution baseline
- [x] Lane 3 - backend trait and native backend decision
- [x] Lane 4 - native write commands
- [x] Lane 5 - native validation baseline
- [x] Lane 5 - full Python validation fixture coverage
- [x] Lane 6 - init and grep command coverage
- [x] Lane 6 - prototype distribution documentation
- [x] Lane 7 - replacement decision
