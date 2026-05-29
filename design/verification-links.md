---
doc_id: verification-links
title: Verification Links & Call-Stack Mapping in Generated Slices
tags: [plan, cli, agents, slicing, v-model]
---

# Verification Links & Call-Stack Mapping in Generated Slices

Status: ready to implement
Supersedes the prose-only `## Verification` scope in [[slice-context-discovery-plan]].

## Why

`slice-context-discovery-plan.md` defined five durable slice body sections and split
the work into lanes A–D. Lanes A (the `context` command + section extraction + config),
B (docs), and D (tests) shipped. **Lane C did not** — the task named in that plan:
*"Reconcile the slice-codebase bundled CLI copy and skill guidance so generated slices
preserve/write the standard headings."*

The result: the CLI can *read and display* `## Runtime Flows` / `## Verification`
(`slice show --call-stacks/--verification`, `slice context`), but the `codebase-slicer`
agent never *writes* them. Only the hand-authored `auth-service.md` example carries the
sections; freshly generated slices have none, so `--verification`/`--call-stacks` print
`(not present)` on every real repo.

This closes Lane C, and upgrades verification from prose to **structured V-model
traceability links** that `slice check` can validate. Unanchored prose is exactly the
doc-staleness failure this tool exists to catch, so verification content that can't be
checked would undercut the premise.

## Design decisions

- **Structured V-model links, not prose.** Each abstraction links *down* to its
  verifying test(s) and *up* to the design doc / requirement it serves.
- **Links live in the body** `## Verification` section as a structured bullet list — not
  YAML frontmatter (contradicts this repo's "slice files stay clean of tracking metadata"
  rule and the [[slice-context-discovery-plan]] non-goal). Renders through the existing
  `extract_sections` + `--verification` path with no CLI display change.
- **`slice check` validates dangling refs by default.** Coverage-gap enforcement
  (abstractions with no link) is opt-in behind `--require-verification`, so default
  `check` stays quiet and trustworthy.
- **Mandatory generated sections:** `## Runtime Flows`, `## Verification`,
  `## Update Triggers`. `## System Behavior` and `## Invariants` stay agent-discretion —
  written only when grounded, never manufactured.

## The `## Verification` link format (the contract)

The agent (writer) and `slice check` (reader) share one shape:

```markdown
## Verification

- `verify_token` <- src/auth/../test_auth.py::test_valid, src/auth/../test_auth.py::test_expired
- `create_session` <- tests/test_sessions.py
- upstream: design/verification-links.md
```

Parsing contract (implemented once, in `slices_cli.py`):

- A `- ` bullet containing ` <- ` splits into `(abstraction, [refs])`. The abstraction is
  the leading token (surrounding backticks stripped). Refs are comma-separated, each a
  `path` or `path::symbol`.
- A `- upstream:` bullet lists one or more design-doc / requirement paths.
- `::symbol` is advisory and not validated — only the *file* is checked, mirroring the
  existing `files[]` existence check.
- Any other text in the section is free prose and ignored by the parser, keeping the
  section human-friendly.

## Scope of changes

1. **`slices_cli.py`** — `parse_verification(body)`; per-slice dangling-ref validation in
   `run_check` (reusing `_resolve_raw_path` + the `files[]` existence pattern); a
   `require_verification` kwarg + `--require-verification` flag (coverage-gap warnings
   comparing frontmatter `abstractions:` against linked abstractions); a `verification`
   case in `_warning_category`.
2. **`agents/codebase-slicer.md`** — refine mode writes `## Runtime Flows` (call-stack
   chains from `outgoingCalls`), `## Verification` (links from `incomingCalls` filtered to
   test files), and `## Update Triggers`; `## System Behavior` / `## Invariants` optional.
3. **`skills/slice-codebase/SKILL.md`** — Phase 2 refine prompt mirrors the agent guidance.
4. **`examples/mock-repo/`** — add `tests/` so links resolve; rewrite `auth-service.md`
   `## Verification` to the structured format as the canonical worked example.
5. **Docs** — `agent-workflows.md`, `README.md`, `CHANGELOG.md`, and mark Lane C done in
   [[slice-context-discovery-plan]].

Test-file heuristic (language-agnostic): path under `tests/`, `test/`, `__tests__/`,
`spec/`, or filename matching `test_*`, `*_test.*`, `*.test.*`, `*.spec.*`.

## Verification

- `pytest test_slices_cli.py` green, including new tests: `parse_verification` extraction;
  valid refs → clean check; dangling test ref → error; dangling upstream ref → error;
  `--require-verification` warns on an uncovered abstraction and is silent without the flag.
- Manual against the mock-repo: `slice --repo examples/mock-repo check` clean;
  `... show auth-service --verification` prints the links; breaking a ref makes `check`
  error; `... check --require-verification` flags the sparse slices.

## Out of scope

- Refreshing qrspi's stale bundled CLI copy.
- Moving any of this into YAML frontmatter.
- `::symbol`-level validation (file existence only, matching `files[]`).
- Generating tests — we only *link* existing ones.
