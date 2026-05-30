---
doc_id: verification-links
title: Verification Links & Call-Stack Mapping
tags: [cli, agents, slicing, v-model, reference]
---

# Verification Links & Call-Stack Mapping

Generated slices carry two durable body sections beyond prose:

- **`## Runtime Flows`** — call-stack chains (e.g. `request -> require_auth ->
  verify_token -> get_session -> handler`), derived from the slice's outgoing calls.
- **`## Verification`** — structured **V-model traceability links**: each abstraction
  links *down* to the test(s) that verify it and *up* to the design doc it serves.

Verification lives as a structured bullet list in the body, not in YAML frontmatter —
slice files stay clean of tracking metadata, and the content renders through the normal
`slice show --verification` / `slice context` path. Because the links are structured,
`slice check` can validate them: unanchored prose is exactly the doc-staleness failure
this tool exists to catch, so verification content that couldn't be checked would
undercut the premise.

This doc is the **single canonical syntax contract**. The `slice-codebase` skill and the
`codebase-slicer` agent link here rather than restating the grammar, so writer and reader
never drift.

## Section headings (the parse boundary)

The card parser splits the body on level-2 headings only. A heading is recognised when the
line **starts with `## ` (hash, hash, single space)**, after which the text is trimmed.
Consequences the writer must respect:

- `## Runtime Flows` is recognised; `##Runtime Flows` (no space) and `### Runtime Flows`
  (level 3) are **not** — that content silently folds into the previous section.
- Use the exact section names the CLI surfaces: `## System Behavior`, `## Invariants`,
  `## Runtime Flows`, `## Verification`, `## Update Triggers`.

## `## Runtime Flows` arrow convention

One call chain per line, joined by ASCII ` -> ` (space, hyphen, greater-than, space):

```markdown
## Runtime Flows

request -> require_auth -> verify_token -> get_session -> handler
```

The body is displayed, not parsed, so a stray bullet won't error — but mixing Unicode
arrows (`→`) or one-flow-across-many-bullets makes `slice show --call-stacks` unreadable.
Stick to ASCII ` -> `, one flow per line, no leading bullet.

## The `## Verification` link format (the contract)

The agent (writer) and `slice check` (reader) share one shape:

```markdown
## Verification

- `verify_token` <- src/auth/../test_auth.py::test_valid, src/auth/../test_auth.py::test_expired
- `create_session` <- tests/test_sessions.py
- upstream: design/auth-spec.md
```

Parsing rules:

- A `- ` bullet containing ` <- ` (space, less-than, hyphen, space) splits into
  `(abstraction, [refs])`. The abstraction is the leading token (surrounding backticks
  stripped). Refs are comma-separated, each a `path` or `path::symbol`.
- A `- upstream:` bullet lists one or more design-doc / requirement paths.
- `::symbol` is advisory and not validated — only the *file* is checked, mirroring the
  `files[]` existence check.
- Any other text in the section is free prose, ignored by the parser, keeping the section
  human-friendly.

## Common pitfalls

| Symptom | Cause | Fix |
|---------|-------|-----|
| A whole section vanishes from `slice show` | `##Heading` (no space) or `### Heading` (level 3) | Use `## ` + a single space |
| `slice check`: "ref not found" | a real path typo in a ` <- ` ref | correct the path; `::symbol` is not checked, the file is |
| A verification link is silently dropped | `→` or `*` instead of ` <- `, or a missing ` <- ` | use the literal ` <- ` separator with a leading `- ` |
| Call chain shows as one blob | Unicode arrows or bullets in `## Runtime Flows` | one flow per line, ASCII ` -> ` |

## Test-file heuristic (language-agnostic)

A ref counts as a test when its path is under `tests/`, `test/`, `__tests__/`, or `spec/`,
or its filename matches `test_*`, `*_test.*`, `*.test.*`, or `*.spec.*`. The agent uses
this to filter incoming calls down to the verifying tests when writing `## Verification`.

## Validation

`slice check` validates verification links by default: a link to a test file or upstream
doc that does not exist is an **error** (same treatment as a dangling `files[]` entry).
Coverage-gap enforcement — flagging an abstraction that has *no* link — is opt-in behind
`slice check --require-verification`, so the default `check` stays quiet and trustworthy.

With `--require-verification`, an abstraction that carries no link is an **error** and
fails the check (non-zero exit), not a warning. The message names the abstraction and the
fix, e.g. ``abstraction not verified: create_session - add `- `create_session` <-
path/to/test::test_name` under ## Verification, or drop create_session from
`abstractions:` if untested``. This is what makes the slicing skill's validate phase a
real gate rather than advisory noise.

The mandatory generated sections are `## Runtime Flows`, `## Verification`, and
`## Update Triggers`. `## System Behavior` and `## Invariants` are written only when the
agent can ground them — never manufactured.
