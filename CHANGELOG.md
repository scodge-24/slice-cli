# Changelog

All notable changes to slice-cli are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/), and the project aims to follow
semantic versioning.

## [0.1.0] — unreleased

First public release.

### Features
- Slice navigation: `list`, `show`, `files`, `deps`, `for`, `find`, `grep`.
- Doc-staleness tracking via a `DOCS.yaml` manifest, anchored on a **content
  fingerprint** of each doc's tracked files (rebase- and sequencing-safe);
  `verified_at` is kept as a human-readable HEAD note.
- `slice context <path-or-slice>` — one-command orientation returning the owning
  slice, linked-doc staleness, and standard system sections from the slice body
  (`System Behavior`, `Invariants`, `Runtime Flows`, `Verification`,
  `Update Triggers`).
- `slice show` section flags: `--body`, `--system`, `--call-stacks`,
  `--verification`.
- **V-model verification links** in the `## Verification` section: structured
  `abstraction <- test::name` traceability links plus an `upstream:` design-doc
  link. `slice check` validates them (dangling test/upstream refs are errors),
  and `slice check --require-verification` warns on abstractions with no link
  (opt-in coverage gap). Format: `design/verification-links.md`.
- `slice affected-docs`, `slice stale-docs`, `slice stamp`, `slice check`,
  `slice sync-index`, `slice docs`, `slice docs-bootstrap`.
- `slice init` — wire slice-cli into a repo (idempotent agent-instruction block
  for CLAUDE.md/AGENTS.md, optional `--hook` and `--ci`).
- `slices/config.yaml` with `context.ambiguity: strict | best_effort`.

### Slice generation (agent side)
- Bundles the `slice-codebase` skill and `codebase-slicer` agent that scan a
  repo and write slice definitions (`/slice-codebase`), no longer tied to a
  private plugin.
- Generated slices now include call-stack mapping (`## Runtime Flows`) and
  V-model verification links (`## Verification`) by default — the refine agent
  derives the links from the `incomingCalls` it already traces, filtered to
  test files. `## Update Triggers` is also written; `System Behavior` /
  `Invariants` stay agent-discretion.
- The repo is an installable Claude Code plugin:
  `claude plugin marketplace add scodge-24/slice-cli`.
- `slice init --agent [--global]` installs the skill + agent into `.claude/`
  (or `~/.claude/`) for the pip-only path.

### Robustness
- Malformed YAML, a missing `git`, and out-of-repo paths now exit 2 with a clear
  message instead of a traceback.

### Known limitations
- No file locking on concurrent `DOCS.yaml` writes (e.g. parallel `slice stamp`).
- Requires `git` on PATH.

### Distribution
- The shipped `slice` command is now a self-contained **Rust binary**
  (`rust/slice-rs`), installable as a prebuilt binary from GitHub Releases or via
  `cargo install --path rust/slice-rs`.
- The Python implementation (`slice_cli`) is retained as the parity test
  **oracle** (run as `python -m slice_cli`) and will be removed once the Rust
  suite is self-sufficient; it no longer installs a `slice` console script.
- BREAKING: there is no longer a Python `slice` console script. Install the Rust
  binary instead. (The earlier single-file `slices_cli` module and root-script
  path were already removed in the package refactor.)
