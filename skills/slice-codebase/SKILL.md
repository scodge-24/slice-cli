---
name: slice-codebase
description: "Generates or refreshes repo slice definitions in slices/. Use when slices are missing, stale, or need updating — or when asked to generate, refresh, update, reslice, or check slices. Aliases: slice, gen-slices, reslice, update-slices, check-slices."
model: sonnet
effort: medium
---

# Slice Codebase

Ensure the repo has current slice definitions, using the `codebase-slicer` subagent for scanning and the `slice` CLI for validation.

Slice cards are a **navigation and context surface** first: once generated, agents and
humans query them with `slice context <file>` (orient on a file), `slice deps <id>
--reverse --transitive` (blast radius before editing), `slice show <id> --call-stacks`
(runtime flows), and `slice find <needle>` (locate a concept). Doc-staleness tracking is
one capability among these. This skill exists to make those queries rich and accurate, so
generation quality matters: a thin or malformed card is a thin answer to every later query.

**Running the slice CLI.** Steps below call `slice <cmd>`. Use `slice` if it is on PATH (the prebuilt binary or `cargo install --path rust/slice-rs`). If it is not, fall back to building from the checkout: `cargo run --manifest-path "$CLAUDE_PLUGIN_ROOT/rust/slice-rs/Cargo.toml" -- <cmd>` (when running as the slice-cli plugin) or `cargo run --manifest-path /path/to/slice-cli/rust/slice-rs/Cargo.toml -- <cmd>` (needs a Rust toolchain).

**Subagent type.** The scan agent is referenced below as `slice-cli:codebase-slicer` (its name when installed via the slice-cli plugin). If slicing was bootstrapped with `slice init --agent` instead, the agent is installed loose as plain `codebase-slicer` — use that name.

If `$ARGUMENTS` contains file or directory paths (e.g., `/slice-codebase src/auth/ src/models/user.rs`), treat them as **scope hints** — the scout's starting search radius. Scope hints narrow where scanning begins without capping the result: the scout still expands outward to include files outside the hints when dependencies cross the boundary.

## Check First

- **Missing** (no `slices/` directory, or no `slices/*.md`): run [Full Slice Generation](#full-slice-generation).
- Otherwise run `slice sync-index --check`. It compares the recorded source fingerprint in
  `slices/INDEX.md` against the current code (content-based, so it survives commits and
  rebases):
  - **exit 0** — slices are in sync with the code. Report "slices up to date" and stop.
  - **exit 1, INDEX.md present** — code drifted: run [Diff Slice Update](#diff-slice-update).
  - **exit 1, no INDEX.md** (slice files exist but the index was deleted): run
    [Full Slice Generation](#full-slice-generation) — run a full generation, since a diff
    update has no base to work from.

Trust `slice sync-index --check` as the source of truth for staleness — it is fingerprint-based,
so rely on it rather than comparing git commit hashes by hand.

---

<important if="slices/ directory is missing or full regeneration was explicitly requested">

## Full Slice Generation

Orchestrate three phases using `subagent_type: "slice-cli:codebase-slicer"` for Phases 1 and 2.

### Phase 1 — Scout (one agent)

Spawn exactly ONE `slice-cli:codebase-slicer` agent. The scout returns candidate boundaries only — no LSP call hierarchies, no deep file reads.

```
Agent call:
  subagent_type: "slice-cli:codebase-slicer"
  prompt: "Scout mode. Scan this repo and return candidate slice boundaries. For each candidate return: id, one-line description, estimated LoC, file list, and entry point files. Map source code only — treat docs/, specs/, *.md, README, config files, and non-source files as out of scope. Keep each whole file in exactly one candidate (files are atomic; a file too large for the target stays one slice). Work from directory structure, file counts, and top-level symbols; you may use LSP workspaceSymbol for a fast symbol overview, and leave call-hierarchy tracing (incomingCalls/outgoingCalls) and in-depth file reads to Phase 2."
```

<important if="scope hints were provided in $ARGUMENTS">
Append to the scout prompt: "Start your scan from these paths as the search origin: <scope hints>. Expand to related files and modules outside these paths when dependencies cross the boundary."
</important>

Wait for results. Review the candidate list for overlapping files, missing areas, and any candidate that splits a single file across slices — merge those into one slice, since files are atomic. Launch Phase 2 once the scout's candidates are in hand.

### Phase 2 — Refine (parallel agents)

Create the `slices/` directory first. Spawn one `slice-cli:codebase-slicer` agent **per candidate** from Phase 1, all in parallel (single message, multiple Agent calls).

Include the full candidate **id→files map** in every refine prompt (every candidate's id and the files it owns) so each agent can resolve a callee file to its owning slice ID and orient dependencies correctly.

```
Agent call (one per candidate, all launched in parallel):
  subagent_type: "slice-cli:codebase-slicer"
  prompt: "Refine mode. Analyze this candidate and write the slice file to slices/<id>.md:
    - candidate-id: <id>
    - files: <file list>
    - entry_points: <entry points>
    - slice map (id → files for every candidate): <id1>=[file, file]; <id2>=[file]; ...
    Use LSP (documentSymbol, outgoingCalls, incomingCalls) to map exports, abstractions, and entry points. Set `dependencies:` to the slices this slice calls INTO (callees from outgoingCalls), resolving each callee file to its owning slice ID via the slice map; reserve `external:` for files in no candidate slice; write `dependencies: []` for a leaf. Name one symbol per abstraction. Write the slice file directly, including the mandatory body sections: ## Runtime Flows (call-stack chains from outgoingCalls), ## Verification (V-model links — for each abstraction, incomingCalls filtered to test files, written as `abstraction <- testpath::testname`, plus an optional `upstream:` design-doc link), and ## Update Triggers. The verification-link format, dependency-direction rule, and test-file heuristic are in the codebase-slicer agent definition."
```

Wait for ALL refine agents to complete before Phase 3.

### Phase 3 — Validate and Index

1. **Validate** — run:
   ```
   slice check --require-verification
   ```
   `--require-verification` makes a card with an unverified abstraction a hard error
   (non-zero exit), so this is a real quality gate, not advisory output.
   - **Pass**: proceed.
   - **Fail**: read the error output, fix the offending slice files, re-run until it passes.
     Common fixes: resolve overlapping file assignments (assign to the slice with stronger
     coupling); merge any single file split across two slices into one slice; fix dangling
     dependency references; correct file paths; and add the missing `` - `abstraction` <- test ``
     line each "abstraction not verified" error names (or drop the abstraction if it is
     genuinely untested).

2. **Self-review each card** before finishing (the full grammar is in the `codebase-slicer`
   agent definition installed alongside this skill). `slice check` gates file overlap and
   verification links, but it does **not** check dependency *direction* — so verify that
   yourself:
   - **Dependency direction — run for every slice.** `slice deps <id>` lists this slice's
     forward dependencies; confirm each one is a slice the card actually calls into (it appears
     as a callee in that card's `## Runtime Flows`). Leaf/utility slices show empty or minimal
     forward deps. Spot-check the inverse with `slice deps <id> --reverse` (which lists callers,
     not callees). This self-review is the only safety net for direction — `slice check` will
     pass a reversed edge.
   - Sections use `## ` + a single space, with the exact names the CLI surfaces.
   - `## Runtime Flows` uses ASCII ` -> `, one flow per line.
   - `## Verification` links read `` - `abstraction` <- path::sym `` with a literal ` <- `, and
     each abstraction names a single symbol matching its link.
   - Every abstraction appears in `## Verification`; every section carries grounded content.

3. **Generate `slices/INDEX.md`** — run:
   ```
   slice sync-index
   ```

4. **Build the semantic index** — so `slice locate` and `slice find --semantic` work on this
   repo from the start (without it, those commands exit 1 with a "no code semantic index; run
   `slice semantic-index --units code` first" hint). This is also the rebuild after a re-slice:
   freshly written cards make any existing index stale.
   ```
   slice semantic-index --units code
   ```
   Needs the `semantic` build feature **and** `OPENROUTER_API_KEY`, and makes one embedding
   API call per ≤96-unit chunk. If the binary lacks the feature or the key is unset, skip the
   build and tell the user the index wasn't built and to run the command above once it is.

</important>

---

<important if="slice sync-index --check reports drift (exit 1) and slices/INDEX.md exists">

## Diff Slice Update

1. Get changed files: `git diff <last-updated-sha> HEAD --name-only`, where
   `<last-updated-sha>` is the `Last updated:` SHA recorded at the top of `slices/INDEX.md`.
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
5. If `slices/SEMANTIC*.json` exists, the embedding index is now stale (its units carry
   per-slice fingerprints): rebuild with `slice semantic-index --units code` when the installed
   binary has the `semantic` feature **and** `OPENROUTER_API_KEY` is set; if either is missing,
   tell the user it needs a rebuild rather than running the command (a keyless run would fail
   with an API error, not a clean message).

The refine phase is skipped for diff updates — directory-level changes only.

</important>

---

## Set up doc tracking (optional)

Slices power navigation on their own. If the repo also has design docs you want kept in
sync with the code, set up `slices/DOCS.yaml` once:

1. Run `slice init --docs <docs-dir>` (e.g. `slice init --docs docs`). When docs carry
   `tracks: [<code paths a doc describes>]` frontmatter, it writes real doc→slice
   mappings; otherwise it writes a commented stub seeded with the docs it found.
2. Add `tracks:` to each design doc's frontmatter (the code paths it documents), then
   re-run `slice init --docs <docs-dir>` (or `slice docs-bootstrap <docs-dir>`).
3. `slice stamp --all` to record baseline fingerprints.

Skip this entirely if the repo only needs navigation — doc-staleness tracking is
orthogonal to slice generation. Ground every `tracks:` mapping in the code the doc actually
describes.

---

## Output

Report:
- Status: generated N slices / updated N slices / already current
- Any validation errors encountered and how they were resolved
- Path to `slices/INDEX.md`

---

## Constraints

- Always issue scout and refine as separate agent calls — each has its own responsibilities and tooling budget.
- Always launch refine agents in parallel (one message, multiple Agent calls).
- Always use the `codebase-slicer` agent for scan and refine work.
- Always run Phase 3 validation after a full generation.
- Drive slicing autonomously to completion — it is mechanical.
