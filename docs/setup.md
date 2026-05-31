# Setting up slice-cli in a repo

This is an **agent-runbook**: the primary reader is an AI agent setting up `slice` in a
target repo with the user's consent. Humans can follow it too — the snippets are
copy-paste ready.

The guiding rule: **`slice` itself only ever writes slice-owned state** (`slices/DOCS.yaml`,
the `INDEX.md` it regenerates). Everything else here — agent instructions, a pre-commit
hook, a CI workflow, the `/slice-codebase` skill — is repo-owner policy. An agent may
apply these, but only after inspecting the repo and asking. Never write them silently.

---

## 0. Agent bootstrap checklist

Before changing anything, inspect the target repo and report what is present vs missing:

```bash
slice --version 2>/dev/null || echo "slice not on PATH"   # is the binary installed?
test -d slices && echo "has slices/"                       # slice cards present?
test -f slices/DOCS.yaml && echo "has DOCS.yaml"           # doc tracking set up?
ls -d docs wiki 2>/dev/null                                # docs worth tracking?
ls -ld CLAUDE.md AGENTS.md 2>/dev/null                     # exist? symlink? (ls -l shows ->)
test -f .git/hooks/pre-commit && echo "has pre-commit hook"
ls .github/workflows/*slice* 2>/dev/null                   # existing slice workflow?
ls -d .claude/skills/slice-codebase .claude/agents 2>/dev/null
```

Then propose a plan split into required / recommended / optional:

```markdown
Required:
- install or locate the `slice` binary
- verify with `slice --repo examples/mock-repo list`

Recommended:
- install the `/slice-codebase` agent support so slices can be generated
- generate or refresh `slices/*.md`

Optional:
- add docs tracking with `slice docs-bootstrap <docs-dir>`
- add a CI workflow
- add a local pre-commit hook
- add repo agent instructions to `CLAUDE.md` and/or `AGENTS.md`
```

**Consent rules:**

- Installing the tool is required, but say which install path you'll use (release
  binary, an existing binary, or a source build).
- Skill/agent install is recommended — ask whether the user wants plugin-based,
  repo-local, or no agent support.
- Docs tracking is optional — enable it only when the repo has docs worth keeping
  fresh against source changes.
- CI, hooks, and agent-instruction edits are **optional policy changes — always ask
  first.**
- **Preserve existing files.** If `CLAUDE.md`, `AGENTS.md`, or `.git/hooks/pre-commit`
  already exist, show the block you intend to insert (or summarize it) before applying,
  and never clobber unrelated content.

Finish by verifying: `slice --version`, `slice list` in the sliced repo, and (if
enabled) `slice check` / `slice stale-docs`.

---

## 1. Install `slice`

`slice` is a single self-contained Rust binary. Pick one:

**Prebuilt release binary** — download the archive for your platform from the
[latest release](https://github.com/scodge-24/slice-cli/releases/latest), unpack it,
and put `slice` on your PATH.

**Build from source** (needs a Rust toolchain):

```bash
git clone https://github.com/scodge-24/slice-cli && cd slice-cli
cargo install --path rust/slice-rs        # installs the `slice` binary
```

Requires `git` on PATH at runtime. `slice browse` additionally needs optional `fzf`
(>= 0.30). **Python/PyPI is not a supported install path** — `slice` is a Rust binary.

Smoke check:

```bash
slice --repo examples/mock-repo list
```

---

## 2. Add slice state to a repo

Generate slice cards under `slices/` with the `/slice-codebase` skill (see §7) or write
them by hand. Then, optionally, set up design-doc staleness tracking:

```bash
slice docs-bootstrap docs   # generate slices/DOCS.yaml from your docs directory
slice stamp --all           # record baseline fingerprints once the mappings look right
```

`slice docs-bootstrap`:

- builds real doc→slice mappings from docs whose frontmatter carries
  `tracks: [<code paths a doc describes>]`;
- otherwise writes a commented **stub** seeded with the docs it found — add `tracks:`
  to each, then re-run with `--force`;
- refuses to clobber an existing `DOCS.yaml` unless you pass `--force`;
- resolves a relative docs dir against the repo root, so `slice --repo <r> docs-bootstrap docs`
  works from anywhere.

---

## 3. Optional: agent instructions

Paste this block into the repo's `CLAUDE.md` and/or `AGENTS.md` so coding agents reach
for `slice` before editing unfamiliar code. Ask first, and preserve existing content.

```markdown
## slice-cli

`slice` (slice-cli) turns this repo's `slices/*.md` cards into a fast query surface for
navigating code - ownership, blast radius, call stacks, concepts - plus tracking whether
design docs have gone stale. Reach for it before editing unfamiliar code.

Navigate:

- `slice context <path>` - orient on a file: its owning slice, dependencies, runtime
  flows, and system notes (add `--json` for linked docs with stale status).
- `slice for <path>` - which slice owns a path.
- `slice show <id> --call-stacks` / `--system` - runtime call chains / full system notes.
- `slice deps <id> --reverse --transitive` - blast radius: every slice that
  (transitively) depends on this one, before you change it.
- `slice find <needle>` - locate a concept or abstraction across slices.

Track doc staleness:

- After changing source, run `slice affected-docs <changed-files>` to see which docs may
  be stale. Update them, then `slice stamp <doc-id>` to mark verified. `slice stale-docs`
  lists everything stale (exit 1 if any).

If `slices/` is missing or out of date, run `/slice-codebase` to (re)generate it.
```

Prefer **repo-level** instructions (`CLAUDE.md` in the repo) when the guidance is
project-specific; use a machine-global `~/.claude/CLAUDE.md` only for cross-repo habits.
If an older slice-cli wrote a `<!-- slice-cli:start --> … <!-- slice-cli:end -->` block,
replace what's between the markers (or delete the block to remove it).

---

## 4. Optional: CI staleness check

Add a workflow that fails when tracked docs go stale. It installs the release binary
directly (no pip). Pin `SLICE_VERSION` to the release you want:

```yaml
# .github/workflows/slice-staleness.yml
name: slice staleness
on: [push, pull_request]
jobs:
  staleness:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0
      - name: Install slice
        env:
          SLICE_VERSION: v0.1.0
        run: |
          curl -fsSL "https://github.com/scodge-24/slice-cli/releases/download/${SLICE_VERSION}/slice-x86_64-unknown-linux-gnu.tar.gz" -o slice.tar.gz
          tar -xzf slice.tar.gz
          mkdir -p "$HOME/.local/bin"
          install -m 0755 slice "$HOME/.local/bin/slice"
          echo "$HOME/.local/bin" >> "$GITHUB_PATH"
      - run: slice check
```

To make stale docs (not just integrity) fail the build, swap `slice check` for
`slice stale-docs` (exit 1 when any doc is stale). The snippet targets
`x86_64-unknown-linux-gnu` (the ubuntu runner); see the
[releases](https://github.com/scodge-24/slice-cli/releases) page for other platforms.

---

## 5. Optional: local pre-commit hook

A warning-only hook (never blocks a commit). The CLI does not install hooks for you —
`.git/hooks/pre-commit` is repo-local policy, and an existing hook must be inspected and
preserved first.

```sh
# .git/hooks/pre-commit  (chmod +x after creating)
#!/bin/sh
# Warns about stale docs; never blocks a commit.
if command -v slice >/dev/null 2>&1; then
    if ! slice stale-docs >/dev/null 2>&1; then
        echo "slice-cli: some tracked docs are stale - run 'slice stale-docs' to review." >&2
    fi
fi
exit 0
```

For a hard gate, replace the warning with `exit 1`. If a hook already exists, append the
`slice stale-docs` check into it instead of overwriting.

---

## 6. Optional: `/slice-codebase` agent support

The `slice-codebase` skill orchestrates a `codebase-slicer` subagent that scans the repo
(via LSP) and writes `slices/*.md` for you. Two ways to install it.

**As a Claude Code plugin (managed, namespaced, auto-updates):**

```bash
claude plugin marketplace add scodge-24/slice-cli
claude plugin install slice-cli@slice-cli
# then install the `slice` binary (see §1) so the skill finds it on PATH
```

**Or install manually (no plugin system):**

Copy `skills/slice-codebase/` and `agents/codebase-slicer.md` from this repo into
`~/.claude/` (every repo on the machine) or a repo's own `.claude/` (that repo only).
Then run `/slice-codebase` in the repo you want sliced.

Generation needs an LSP-capable environment. Generated slices carry call-stack mapping
(`## Runtime Flows`) and V-model verification links (`## Verification`, validated by
`slice check`) by default — see [`verification-links.md`](verification-links.md).
