# Changelog

All notable changes to slice-cli are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/), and the project aims to follow
semantic versioning.

## [0.2.1] — 2026-06-01

### Changed
- **`/slice-codebase` produces correct slice cards.** Hardened the bundled
  `codebase-slicer` agent and `slice-codebase` skill after a full re-slice surfaced
  recurring card defects:
  - `dependencies:` now means **callees** (slices this one calls into, derived from
    `outgoingCalls`); callers feed the reverse view (`slice deps --reverse`) and the
    verification links. Reversed edges had been passing `slice check`, which validates
    id existence, not direction — so Phase 3 now runs `slice deps` per slice as a
    deterministic direction self-review.
  - A single source file always belongs to exactly one slice; an oversized file stays
    whole rather than being split by line range.
  - Each `abstractions:` entry names one symbol, so it matches its verification link
    (slash-joined `A / B / C` entries matched none of theirs).

### For contributors
- Converted the agent/skill prompts from prohibition ("do not / never") to affirmative
  directives, per Opus 4.8 prompting guidance, and fixed a duplicate "Step 6" heading in
  the agent definition. Prompts only — no `rust/slice-rs/` change.

## [0.2.0] — 2026-05-31

### Changed
- **BREAKING: `slice init` is removed.** The CLI no longer writes agent instructions
  (`CLAUDE.md` / `AGENTS.md`), git hooks, GitHub Actions workflows, or loose
  `.claude/` skill/agent files — it writes only slice-owned state. Set up
  `slices/DOCS.yaml` with `slice docs-bootstrap <dir>` (formerly `slice init --docs`).
- `slice docs-bootstrap` absorbs the former `init --docs` behavior: it writes a
  commented stub when no doc carries `tracks:` yet, resolves a relative docs dir
  against the repo root (not the process CWD), and regenerates an existing manifest
  with `--force`.
- Optional CI / hook / agent-instruction / skill setup now lives in an agent-run
  runbook, [`docs/setup.md`](docs/setup.md), applied with the repo owner's consent
  rather than by the binary.

### Migration
Repos that ran the old `slice init` can remove the generated files by hand:
- delete the `<!-- slice-cli:start -->` block from `CLAUDE.md` / `AGENTS.md`
- delete `.github/workflows/slice-staleness.yml` if you don't want it
- inspect `.git/hooks/pre-commit` before removing it

## [0.1.0] — 2026-05-31

First public release.

### Features
- Slice navigation: `list`, `show`, `files`, `deps`, `for`, `find`, `grep`.
- `slice show` renders the slice's **overview** — the prose lede before the first
  `##` section — as a block under `description` (and as an `overview` field in
  `--json`), so the slice's orientation prose is visible without `--body`.
- `slice browse` — an interactive `fzf` picker (optional `fzf` >= 0.30 dependency)
  fed from structured slice data, with a live wrapped preview pane and lens keys:
  `ctrl-o` overview, `ctrl-r` runtime call-stacks, `ctrl-d` verification, `ctrl-t`
  reverse deps. `enter` shows the slice; `--print` emits the selected id for piping
  (`id=$(slice browse --print) && slice show "$id"`).
- **Terminal color** on human output (`list`/`show`/`find`/`stale-docs`), gated by a
  global `--color=auto|always|never` flag. `auto` (the default) colors only when
  stdout is a terminal and honors `NO_COLOR`; pipes and the `--json` path are never
  colored, so terminal output now carries color by default while scripts and agents
  see byte-identical plain text. `slice list` gains an at-a-glance `[N stale]` badge.
- Doc-staleness tracking via a `DOCS.yaml` manifest, anchored on a **content
  fingerprint** of each doc's tracked files (rebase- and sequencing-safe);
  `verified_at` is kept as a human-readable HEAD note.
- `slice context <path-or-slice>` — one-command orientation returning the owning
  slice, linked-doc staleness, and standard system sections from the slice body
  (`System Behavior`, `Invariants`, `Runtime Flows`, `Verification`,
  `Update Triggers`).
- `slice show` section flags: `--body`, `--system`, `--call-stacks`,
  `--verification`.
- `slice deps <id> --reverse --transitive` walks the full transitive blast
  radius — every slice that depends on `<id>` directly or through intermediaries
  — so you can see everything a change touches before you make it.
- **V-model verification links** in the `## Verification` section: structured
  `abstraction <- test::name` traceability links plus an `upstream:` design-doc
  link. `slice check` validates them (dangling test/upstream refs are errors),
  and `slice check --require-verification` is an opt-in coverage **gate**: an
  abstraction with no link is an error (non-zero exit) with a fix-hint message,
  so the slice-generation skill can rely on it. Format:
  `docs/verification-links.md` is the canonical card-syntax contract.
- `slice affected-docs`, `slice stale-docs`, `slice stamp`, `slice check`,
  `slice sync-index`, `slice docs`, `slice docs-bootstrap`.
- `slice init` — wire slice-cli into a repo (idempotent agent-instruction block
  for CLAUDE.md/AGENTS.md, optional `--hook` and `--ci`). `--docs <dir>` sets up
  doc tracking: it bootstraps `DOCS.yaml` from docs whose frontmatter carries
  `tracks:` and otherwise writes a commented stub seeded with the docs it found,
  never clobbering an existing manifest.
- The injected CLAUDE.md block, the `slice-codebase` skill, and the
  `codebase-slicer` agent lead with navigation (ownership, blast radius, call
  stacks, concepts); doc-staleness is one capability among them. Docs are
  documentation-system-agnostic (no Obsidian assumption), and the `DOCS.yaml`
  top-level key is `docs_root` (a legacy `vault_root` alias still loads).
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
- The Python implementation has been fully removed from the tree once the Rust
  suite reached native parity (50 tests, no oracle). It remains recoverable at
  tag `python-impl-final` and branch `package-refactor`.
- BREAKING: there is no longer a Python `slice` console script. Install the Rust
  binary instead. (The earlier single-file `slices_cli` module and root-script
  path were already removed in the package refactor.)
