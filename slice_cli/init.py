from __future__ import annotations

import argparse
from pathlib import Path

from .context import Ctx


_INIT_BLOCK_START = "<!-- slice-cli:start -->"


_INIT_BLOCK_END = "<!-- slice-cli:end -->"


_AGENT_INSTRUCTIONS = """\
## slice-cli

This repo tracks whether design docs are stale relative to the code they
describe, via `slice` (slice-cli) and `slices/DOCS.yaml`.

- Before editing an unfamiliar file, run `slice context <path>` to see the
  owning slice, its system context, and any stale linked docs.
- After changing source, run `slice affected-docs <changed-files>` to see which
  docs may need updating. Update stale docs, then `slice stamp <doc-id>` to mark
  them verified.
- `slice stale-docs` lists everything currently stale (exit 1 if any are stale).
- If `slices/` is missing or out of date, run `/slice-codebase` to (re)generate
  slice definitions.
"""


_HOOK_SCRIPT = """\
#!/bin/sh
# Installed by `slice init --hook`. Warns about stale docs; never blocks a commit.
if command -v slice >/dev/null 2>&1; then
    if ! slice stale-docs >/dev/null 2>&1; then
        echo "slice-cli: some tracked docs are stale — run 'slice stale-docs' to review." >&2
    fi
fi
exit 0
"""


_CI_WORKFLOW = """\
name: slice staleness
on: [push, pull_request]
jobs:
  staleness:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0
      - uses: actions/setup-python@v5
        with:
          python-version: "3.12"
      # Install the slice CLI. Swap for a pinned version or a git source as needed:
      #   pip install slice-cli==0.1.0
      #   pip install git+https://github.com/scodge-24/slice-cli
      - run: pip install slice-cli
      - run: slice check
"""


_SLICE_CODEBASE_SKILL = """\
---
name: slice-codebase
description: "Generates or refreshes repo slice definitions in slices/. Use when slices are missing, stale, or need updating — or when asked to generate, refresh, update, reslice, or check slices. Aliases: slice, gen-slices, reslice, update-slices, check-slices."
model: sonnet
effort: medium
---

# Slice Codebase

Ensure the repo has current slice definitions, using the `codebase-slicer` subagent for scanning and the `slice` CLI for validation.

**Running the slice CLI.** Steps below call `slice <cmd>`. Use `slice` if it is on PATH (the `pip install slice-cli` entry point). If it is not, fall back to the bundled package from the plugin checkout: `PYTHONPATH="$CLAUDE_PLUGIN_ROOT" python3 -m slice_cli <cmd>` (when running as the slice-cli plugin) or `PYTHONPATH=/path/to/slice-cli python3 -m slice_cli <cmd>`.

**Subagent type.** The scan agent is referenced below as `slice-cli:codebase-slicer` (its name when installed via the slice-cli plugin). If slicing was bootstrapped with `slice init --agent` instead, the agent is installed loose as plain `codebase-slicer` — use that name.

If `$ARGUMENTS` contains file or directory paths (e.g., `/slice-codebase src/auth/ src/models/user.rs`), treat them as **scope hints** — the scout's starting search radius. Scope hints narrow where scanning begins but do not cap the result: the scout must still expand outward to include files outside the hints when dependencies cross the boundary.

## Check First

- Look for `slices/INDEX.md` at the repo root.
- **Up to date**: read the commit hash from INDEX.md, compare against `git rev-parse HEAD`. If they match, report "slices up to date" and stop.
- **Stale** (INDEX.md exists but hash is old): run [Diff Slice Update](#diff-slice-update).
- **Missing** (no INDEX.md): run [Full Slice Generation](#full-slice-generation). This covers the edge case where `slices/*.md` files exist but the index was deleted — do not attempt a diff update with no base hash.

---

<important if="slices/ directory is missing or full regeneration was explicitly requested">

## Full Slice Generation

Orchestrate three phases using `subagent_type: "slice-cli:codebase-slicer"` for Phases 1 and 2.

### Phase 1 — Scout (one agent)

Spawn exactly ONE `slice-cli:codebase-slicer` agent. The scout returns candidate boundaries only — no LSP call hierarchies, no deep file reads.

```
Agent call:
  subagent_type: "slice-cli:codebase-slicer"
  prompt: "Scout mode. Scan this repo and return candidate slice boundaries. For each candidate return: id, one-line description, estimated LoC, file list, and entry point files. Source code only — ignore docs/, specs/, *.md, README, config files, and non-source files. Do NOT read file contents in depth. You may use LSP workspaceSymbol for a fast symbol overview, but do NOT trace call hierarchies (incomingCalls/outgoingCalls) — that is Phase 2 work. Directory structure, file counts, and top-level symbols only."
```

<important if="scope hints were provided in $ARGUMENTS">
Append to the scout prompt: "Start your scan from these paths as the search origin: <scope hints>. Expand to related files and modules outside these paths when dependencies cross the boundary."
</important>

Wait for results. Review the candidate list for obvious issues (overlapping files, missing areas). Do NOT launch Phase 2 until scout results are in hand.

### Phase 2 — Refine (parallel agents)

Create the `slices/` directory first. Spawn one `slice-cli:codebase-slicer` agent **per candidate** from Phase 1, all in parallel (single message, multiple Agent calls).

Include the full candidate ID list in every refine prompt so agents can map cross-slice dependencies correctly.

```
Agent call (one per candidate, all launched in parallel):
  subagent_type: "slice-cli:codebase-slicer"
  prompt: "Refine mode. Analyze this candidate and write the slice file to slices/<id>.md:
    - candidate-id: <id>
    - files: <file list>
    - entry_points: <entry points>
    - all candidate IDs: <full list from scout>
    Use LSP (documentSymbol, outgoingCalls, incomingCalls) to map exports, abstractions, entry points, and cross-slice dependencies. Write the slice file directly, including the mandatory body sections: ## Runtime Flows (call-stack chains from outgoingCalls), ## Verification (V-model links — for each abstraction, incomingCalls filtered to test files, written as `abstraction <- testpath::testname`, plus an optional `upstream:` design-doc link), and ## Update Triggers. The verification-link format and test-file heuristic are in the codebase-slicer agent definition and design/verification-links.md."
```

Wait for ALL refine agents to complete before Phase 3.

### Phase 3 — Validate and Index

1. **Validate** — run:
   ```
   slice check
   ```
   - **Pass**: proceed.
   - **Fail**: read the error output, fix the offending slice files, re-run until it passes.
     Common fixes: resolve overlapping file assignments (assign to the slice with stronger coupling), fix dangling dependency references, correct file paths.

2. **Generate `slices/INDEX.md`** — run:
   ```
   slice sync-index
   ```

</important>

---

<important if="slices/INDEX.md exists but its commit hash is behind HEAD">

## Diff Slice Update

1. Get changed files: `git diff <index-hash> HEAD --name-only`
2. Spawn one `slice-cli:codebase-slicer` agent in scout mode, passing only the changed files:

```
Agent call:
  subagent_type: "slice-cli:codebase-slicer"
  prompt: "Scout mode. Only look at these changed files and determine which existing slices need updating, and whether any new slices are needed: <changed file list>. Source code only."
```

<important if="scope hints were provided in $ARGUMENTS">
Filter the changed file list to files within the hinted paths before passing to the scout. If no changed files fall within the hints, pass the full changed file list instead.
</important>

3. For each affected slice file: update its `files` list to reflect additions/deletions, adjust `entry_points` if affected, and fix any `dependencies` references to new or renamed slices. Then regenerate the index:
   ```
   slice sync-index
   ```
4. Run `slice check` to validate. Fix any errors before finishing.

The refine phase is skipped for diff updates — directory-level changes only.

</important>

---

## Output

Report:
- Status: generated N slices / updated N slices / already current
- Any validation errors encountered and how they were resolved
- Path to `slices/INDEX.md`

---

## Constraints

- Never combine scout and refine work into one agent call — they have different responsibilities and tooling budgets.
- Never run refine agents sequentially — always parallel.
- Never use Explore, general-purpose, or any other agent type — always the `codebase-slicer` agent.
- Never skip validation (Phase 3) after full generation.
- Never ask the user for input during slicing — it is mechanical.
"""


_CODEBASE_SLICER_AGENT = """\
---
name: codebase-slicer
description: Maps codebase structure into slice boundaries. Two modes — "scout" scans the whole repo and returns candidate slice boundaries; "refine" takes one candidate area and uses LSP to map its exports, entry points, and dependencies. Call with mode and scope in the prompt.
tools: Read, Grep, Glob, LS, LSP, Write
model: sonnet
---

You are a specialist at mapping codebase structure. You operate in one of two modes, specified in your prompt.

## CRITICAL: YOU ARE A DOCUMENTARIAN

- DO NOT suggest improvements or changes to the codebase
- DO NOT critique architecture, code quality, or design decisions
- DO NOT perform root cause analysis or identify problems
- ONLY map what exists, where it exists, and how components relate
- DO NOT spawn sub-agents — you are the sub-agent
- **ONLY map source code** — ignore docs/, specs/, README*, *.md, CHANGELOG, LICENSE, and any other non-source files. Slices are for code that agents will research and modify, not documentation or specifications.

<important if="your prompt says 'scout' or 'scan the repo'">

## Scout Mode

Your job: map the whole repo into candidate slice boundaries. Fast and shallow.

**Steps:**

1. `LS` the repo root and major directories to understand the top-level shape
2. `Glob` for source files to estimate LoC per area — only source code (*.py, *.ts, *.js, *.go, *.rs, etc.), skip docs, configs, markdown, and non-code files
3. Optionally use `LSP workspaceSymbol` for a fast overview of top-level symbols across the repo — helps identify module boundaries and key abstractions without reading files
4. Identify candidate slice boundaries based on directory structure, symbol overview, and module organization — skip directories that are purely documentation (docs/, wiki/, etc.)

**Boundary strategy:**
- Start with directories — the file system is the strongest signal for module boundaries
- Target 500-2000 LoC per slice
- Merge small modules (<~500 LoC) with their closest dependency
- Split large modules (~2000+ LoC) along natural seams visible from directory structure
- Exception: a tightly cohesive module stays as one slice even if it exceeds the target

**Output format** — return a structured list of candidates:

```
## Candidate Slices

### <candidate-id>
- description: <one-line summary>
- estimated_loc: <number>
- files:
  - <path>
  - <path>
- entry_points: <main files or directories to investigate with LSP>
- split_reason: <why this is a separate slice, if not obvious from directory structure>

### <candidate-id>
...
```

**Do NOT** trace call hierarchies (incomingCalls/outgoingCalls) in scout mode — that's refine work.
**Do NOT** read file contents in depth.
**Do NOT** write any files — return your candidates as output text only.
This must be fast — directory structure, file counts, and optionally workspaceSymbol only.

</important>

<important if="your prompt says 'refine' and specifies a candidate slice area">

## Refine Mode

Your job: take one candidate slice area (provided in your prompt with its file list) and use LSP to map its internals and validate the boundary.

**Steps:**

1. Use `documentSymbol` on key files to identify exports, classes, and public interfaces
2. Use `outgoingCalls`/`incomingCalls` on entry points to map the real dependency graph
3. Check whether the candidate boundary matches the actual call graph
4. Identify abstractions that define this slice's public interface
5. Identify dependencies on other areas of the codebase (by file path — the orchestrator will map these to slice IDs)
6. Note any files that should be excluded (tests, generated code, etc.)
7. **Map verification links.** For each public abstraction, take its `incomingCalls` and keep callers that live in **test files** — those are the tests that verify it. A file is a test file if its path is under `tests/`, `test/`, `__tests__/`, or `spec/`, or its name matches `test_*`, `*_test.*`, `*.test.*`, or `*.spec.*`. Record each as `abstraction <- testpath::testname`.

**Step 6: Write the slice file.** Write your findings directly to `slices/<slice-id>.md` using this exact format:

```yaml
---
slice_id: <kebab-case-id>
description: "<one-line summary>"
loc: <estimated lines of code>
files:
  - <path relative to repo root>
abstractions:
  - "<ExportedName — what it does>"
exclusions:
  - <paths explicitly not in this slice>
dependencies:
  - <other slice IDs this slice depends on>
---

# <Slice Title>

<2-5 sentences: what this slice covers, main entry points, key data flows.
Note any boundary decisions (e.g., why LoC exceeds target, why files are grouped this way).>

## Runtime Flows

<Call-stack chains from the entry points, derived from outgoingCalls. One chain per line, e.g.:
request -> require_auth -> verify_token -> get_session -> handler>

## Verification

<V-model traceability links. One bullet per verified abstraction (from Step 7), plus an
optional `upstream:` link to the design doc/requirement this slice serves. Omit the
upstream line if no design doc clearly covers this slice. Format:
- `abstraction` <- path/to/test_file::test_name, path/to/test_file::other_test
- upstream: design/<relevant-doc>.md>

## Update Triggers

<What should trigger re-verification of this slice — typically its own entry points and
public contracts. When these change, the linked tests must be re-run and this slice
re-reviewed.>
```

The three sections above (`Runtime Flows`, `Verification`, `Update Triggers`) are
**mandatory** — they carry the call-stack map and the verification links the CLI surfaces
via `slice show --call-stacks` / `--verification`. Optionally add `## System Behavior` and
`## Invariants` when you can ground them in what the code actually does; skip them rather
than guessing. The verification-link format is specified in `design/verification-links.md`;
`slice check` validates the links, so every referenced path must exist.

For dependencies, use slice IDs (provided in your prompt alongside the candidate list), not file paths. If a dependency target isn't in the candidate list, note the file path and prefix with `external:`.

Read files selectively — focus on entry points and public interfaces, not every line.

</important>

## What NOT to Do

- Don't analyze implementation details beyond what's needed for boundary decisions
- Don't suggest better code organization
- Don't comment on code quality or architecture decisions
- Don't create slices for test directories — note them in exclusions
- Don't create slices smaller than ~200 LoC unless they're genuinely independent
- Don't spawn sub-agents — you are the sub-agent
- Don't include documentation, specs, or non-source files (*.md, docs/, specs/, README, CHANGELOG, LICENSE) in slices — slices are for source code only
- Don't create slices for config files, CI pipelines, or build scripts unless they contain substantial logic
"""


def _render_agent_block() -> str:
    return f"{_INIT_BLOCK_START}\n{_AGENT_INSTRUCTIONS}{_INIT_BLOCK_END}\n"


def _upsert_block(existing: str, block: str) -> str:
    """Insert or replace the marked slice-cli block. Idempotent."""
    if _INIT_BLOCK_START in existing and _INIT_BLOCK_END in existing:
        start = existing.index(_INIT_BLOCK_START)
        end = existing.index(_INIT_BLOCK_END) + len(_INIT_BLOCK_END)
        # consume a trailing newline after the end marker if present
        tail = existing[end:]
        if tail.startswith("\n"):
            tail = tail[1:]
        return existing[:start] + block + tail
    sep = "" if existing == "" or existing.endswith("\n\n") else ("\n" if existing.endswith("\n") else "\n\n")
    return existing + sep + block


def cmd_init(args: argparse.Namespace, ctx: Ctx) -> int:
    """Wire slice-cli into a repo: agent-instruction block, optional hook + CI,
    optional slicing skill + agent (--agent)."""
    root = ctx.repo_root
    block = _render_agent_block()
    planned: list[tuple[Path, str]] = []

    # Agent-instruction files: always CLAUDE.md; AGENTS.md too if it exists.
    agent_files = [root / "CLAUDE.md"]
    if (root / "AGENTS.md").exists():
        agent_files.append(root / "AGENTS.md")
    if args.global_:
        agent_files = [Path.home() / ".claude" / "CLAUDE.md"]
    for path in agent_files:
        existing = path.read_text(encoding="utf-8") if path.exists() else ""
        planned.append((path, _upsert_block(existing, block)))

    if args.hook:
        planned.append((root / ".git" / "hooks" / "pre-commit", _HOOK_SCRIPT))
    if args.ci:
        planned.append((root / ".github" / "workflows" / "slice-staleness.yml", _CI_WORKFLOW))

    if args.agent:
        # Install the slice-codebase skill + codebase-slicer agent so an agent can
        # generate slices here. --global puts them in ~/.claude (usable in every
        # repo); otherwise project-local .claude/. Loose installs are not
        # namespaced, so the skill must reference the bare `codebase-slicer`.
        base = Path.home() if args.global_ else root
        loose_skill = _SLICE_CODEBASE_SKILL.replace(
            "slice-cli:codebase-slicer", "codebase-slicer"
        )
        planned.append((base / ".claude" / "skills" / "slice-codebase" / "SKILL.md", loose_skill))
        planned.append((base / ".claude" / "agents" / "codebase-slicer.md", _CODEBASE_SLICER_AGENT))

    if args.dry_run:
        for path, _ in planned:
            print(f"would write: {ctx.rel(path)}")
        return 0

    for path, content in planned:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content, encoding="utf-8")
        if path.name == "pre-commit":
            path.chmod(0o755)
        print(f"wrote {ctx.rel(path)}")
    return 0
