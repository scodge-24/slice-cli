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
- `slice affected-docs`, `slice stale-docs`, `slice stamp`, `slice check`,
  `slice sync-index`, `slice docs`, `slice docs-bootstrap`.
- `slice init` — wire slice-cli into a repo (idempotent agent-instruction block
  for CLAUDE.md/AGENTS.md, optional `--hook` and `--ci`).
- `slices/config.yaml` with `context.ambiguity: strict | best_effort`.

### Slice generation (agent side)
- Bundles the `slice-codebase` skill and `codebase-slicer` agent that scan a
  repo and write slice definitions (`/slice-codebase`), no longer tied to a
  private plugin.
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
- Installable via `pip`/`pipx` as the `slice` console script. PyPI publish is
  planned but not yet done.
