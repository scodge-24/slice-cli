---
doc_id: slice-context-discovery-plan
title: Slice Context Discovery Plan
tags: [plan, cli, agents, dx]
---

# Slice Context Discovery Plan

Status: reviewed and ready to implement
Generated: 2026-05-29

## GStack Artifacts

- [Office-hours design doc](</home/scodge/.gstack/projects/scodge-24-slice-cli/scodge-main-design-20260529-095139.md>)
- [Eng-review task artifact](</home/scodge/.gstack/projects/scodge-24-slice-cli/tasks-eng-review-20260529-095557.jsonl>)
- [DevEx-review task artifact](</home/scodge/.gstack/projects/scodge-24-slice-cli/tasks-devex-review-20260529-101151.jsonl>)

## Summary

Add first-class CLI discovery for slice body context so agents do not stop at
metadata. The primary user is an AI coding agent editing unfamiliar code; the
secondary user is an OSS human maintainer or adopter.

Target DX: Champion tier, under 2 minutes from "I have a file path" to useful
system context.

```text
agent has a file path
  -> slice context <path>
  -> sees owner, stale docs, system behavior, call stacks, verification
  -> edits with the right slice context
```

This keeps durable system knowledge in `slices/*.md`, not a new docs layer.

## Key Changes

> **Lane C update:** The generation side of this plan (making the `codebase-slicer`
> agent actually write these headings) shipped separately — see
> [[verification-links]], which also upgrades `## Verification` from prose to
> structured, `slice check`-validated V-model traceability links.

- Add standard Markdown headings for durable slice body content:
  - `## System Behavior`
  - `## Invariants`
  - `## Runtime Flows`
  - `## Verification`
  - `## Update Triggers`
- Add `slice context <path-or-slice>`:
  - resolves file paths through existing owner lookup
  - resolves slice ids/doc stems through existing slice lookup
  - prints slice id, description, doc path, files/deps summary, linked docs with
    stale/current status when `DOCS.yaml` exists, then standard body sections
  - supports `--json`
- Add `slice show` flags:
  - `--body`: full Markdown body
  - `--system`: all standard system sections
  - `--call-stacks`: `Runtime Flows` only
  - `--verification`: `Verification` and `Update Triggers`
- Add `slices/config.yaml`:

```yaml
context:
  ambiguity: strict # strict | best_effort
```

CLI flags override config: `slice context <path> --strict` or
`--best-effort`. Missing config defaults to strict.

## DevEx Requirements

- `slice -h` must show the `context` command.
- `slice context -h` must include copy-paste examples:

```bash
slice context apps/backend/src/services/meal-logs.ts
slice context backend-meal-logs --call-stacks
slice context apps/backend/src/services/meal-logs.ts --best-effort
```

- `slice show -h` must show the section flags and at least one example:

```bash
slice show backend-meal-logs --call-stacks
slice show backend-meal-logs --verification
```

- `slice context -h` must document:
  - config path: `slices/config.yaml`
  - default: strict ambiguity handling
  - override flags: `--strict`, `--best-effort`
- Add a README quickstart for OSS users:
  - what `slice` does
  - one example using `slice context <path>`
  - test command: `python3 -m pytest test_slices_cli.py`

## Implementation Details

- Implement one shared `extract_sections(body)` helper:
  - parse only level-2 Markdown headings: `## Heading`
  - match standard section names case-insensitively
  - preserve section text, trimming only outer blank lines
  - return empty sections when headings are absent
- Use the same renderer for `slice context` and `slice show` section flags.
- Human missing-section output:

```text
Runtime Flows: (not present)
```

- JSON missing-section output:

```json
{
  "sections": {}
}
```

- Strict ambiguity behavior:
  - no owner: exit nonzero with `no owning slice for: <path>`
  - multiple owners: exit nonzero and list matching slice ids
- Best-effort ambiguity behavior:
  - no owner: same nonzero error
  - multiple owners: print all matching slices in deterministic slice-id order
- Invalid config value should fail with a clear error naming the bad value,
  allowed values, and config path.
- Update `design/agent-workflows.md` with the new orientation flow.
- Reconcile the `slice-codebase` bundled CLI copy and skill guidance so
  generated slices preserve/write the standard headings.

## Test Plan

Coverage diagram:

```text
CODE PATHS                                      USER/AGENT FLOWS
[+] section extraction                          [+] Agent starts from source file
  |-- heading found                               |-- context resolves one owner
  |-- heading missing                             |-- no owner returns clear error
  `-- unrelated headings ignored                  `-- multi-owner follows config/flag

[+] slice show flags                            [+] Agent asks for precise context
  |-- --body                                      |-- --call-stacks visible in help
  |-- --system                                    `-- --verification visible in help
  |-- --call-stacks
  `-- --verification

[+] config loading                              [+] Existing doc staleness flow
  |-- config absent defaults strict               |-- DOCS.yaml absent degrades cleanly
  |-- strict configured                           `-- linked docs show stale/current
  `-- best_effort configured

[+] help and README                             [+] OSS adopter evaluates quickly
  |-- context examples visible                    |-- quickstart gives first command
  `-- config behavior documented                  `-- tests are discoverable
```

Required tests in `test_slices_cli.py`:

- `slice show auth-service --body` includes full body.
- `slice show auth-service --system` includes only standard sections.
- `slice show auth-service --call-stacks` prints only `Runtime Flows`.
- `slice show auth-service --verification` prints `Verification` and
  `Update Triggers`.
- `slice context src/auth/middleware.py` resolves `auth-service`.
- `slice context auth-service --json` returns stable section JSON.
- Missing sections do not fail.
- Missing `slices/config.yaml` defaults to strict.
- Config `context.ambiguity: best_effort` allows multi-owner output.
- CLI `--strict` overrides best-effort config.
- Invalid config value fails with allowed values and config path.
- CLI help advertises `context`, examples, `--call-stacks`, `--verification`,
  and config behavior.

Run:

```bash
python3 -m pytest test_slices_cli.py
./slices_cli.py -h
./slices_cli.py show -h
./slices_cli.py context -h
```

## Review Outcomes

What already exists:

- `SliceDoc.body` is already loaded.
- `slice find` already searches body text.
- `DOCS.yaml` already models linked docs and staleness.
- `argparse` subcommands already drive CLI help and dispatch.

Not in scope:

- Enforcing required body headings in `slice check`.
- Moving system context into YAML frontmatter.
- Creating a new docs/wiki layer.
- Replacing `DOCS.yaml` staleness tracking.
- Building a rich Markdown parser.
- Adding a separate `slice config` command in v1.

Failure modes to cover:

- Silent wrong context from ambiguous file ownership: strict default plus tests.
- Agents cannot discover the feature: `-h` examples and help tests.
- Config typo silently changes behavior: explicit config error.
- Bundled skill CLI drift: explicit reconciliation task.
- OSS user cannot evaluate the tool: README quickstart.

Parallelization:

```text
Lane A: core CLI command/section extraction/config (sequential)
Lane B: README + design/agent-workflows docs (after command shape fixed)
Lane C: slice-codebase bundled CLI/generation guidance (after command shape fixed)
Lane D: tests (with Lane A, then expanded after B/C)
```

Recommended execution: implement Lane A + tests together, then update docs and
bundled skill copy.

## GStack Review Report

| Review       | Trigger                | Why                  | Runs | Status  | Findings                                                                  |
| ------------ | ---------------------- | -------------------- | ---- | ------- | ------------------------------------------------------------------------- |
| Office Hours | `/office-hours`        | Problem framing      | 1    | APPROVED | one-command context chosen, precise flags retained                        |
| Eng Review   | `/plan-eng-review`     | Architecture & tests | 1    | CLEAR   | 3 issues resolved: ambiguity policy, config path, bundled CLI drift       |
| DX Review    | `/plan-devex-review`   | DX gaps              | 1    | CLEAR   | score: 7/10 -> 9/10, TTHW target: <2 min                                  |
| CEO Review   | `/plan-ceo-review`     | Scope & strategy     | 0    | not run | not needed for tooling affordance                                         |
| Design Review| `/plan-design-review`  | UI/UX gaps           | 0    | not run | no UI scope                                                               |

- Unresolved: 0
- Verdict: Office Hours + Eng + DX cleared; ready to implement.
