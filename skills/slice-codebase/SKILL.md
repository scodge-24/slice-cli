---
name: slice-codebase
description: "Generates or refreshes repo slice definitions in slices/. Use when slices are missing, stale, or need updating — or when asked to generate, refresh, update, reslice, or check slices. Aliases: slice, gen-slices, reslice, update-slices, check-slices."
model: sonnet
effort: medium
---

# Slice Codebase

Ensure the repo has current slice definitions, using the `codebase-slicer` subagent for scanning and the `slice` CLI for validation.

**Running the slice CLI.** Steps below call `slice <cmd>`. Use `slice` if it is on PATH (the prebuilt binary or `cargo install --path rust/slice-rs`). If it is not, fall back to building from the checkout: `cargo run --manifest-path "$CLAUDE_PLUGIN_ROOT/rust/slice-rs/Cargo.toml" -- <cmd>` (when running as the slice-cli plugin) or `cargo run --manifest-path /path/to/slice-cli/rust/slice-rs/Cargo.toml -- <cmd>` (needs a Rust toolchain).

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
