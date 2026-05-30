---
doc_id: package-refactor-plan
title: Slice CLI Package Refactor Plan
tags: [plan, cli, packaging, refactor]
---

# Slice CLI Package Refactor Plan

Status: reviewed (eng) and ready to implement
Generated: 2026-05-29
Last reviewed: 2026-05-29 (plan-eng-review)

## Summary

Refactor `slices_cli.py` (2,454 lines, single file) into a `slice_cli/` package
at the repo root. This is a clean cut, not a compatibility exercise — legacy
`import slices_cli` is intentionally dropped.

User-facing behavior that stays stable:

- `slice` still works after install.
- `python -m slice_cli ...` works from a fresh checkout (no install required).
- Command behavior, JSON shapes, exit codes, and template install behavior stay
  identical during the refactor.

Explicitly NOT preserved (eng review decision):

- `import slices_cli as cli` — dropped. There are no known external consumers and
  the refactor exists to remove the single-file constraint.
- `python slices_cli.py ...` — replaced by `python -m slice_cli ...`. The root
  `slices_cli.py` file is deleted.

The goal is contributor navigation, not line-count reduction. A contributor
should be able to find persistence, drift detection, validation, init/templates,
bootstrap, or CLI wiring without reading the whole module.

## Chosen Approach

Use a layered package migration with a **flat, repo-root package layout** (no
`src/`), so the package is importable from a bare clone with zero path tricks:

1. Add the `slice_cli/` package (repo root) with `__init__.py`, `__main__.py`,
   and `cli.py`. Point the console script at `slice_cli.cli:main`.
2. Move data models, context, persistence, and shared path/fingerprint helpers.
3. Move doc drift and validation behind stable module interfaces.
4. Move command use cases/rendering and argparse/main wiring.
5. Move `cmd_docs_bootstrap` (464 lines) into its own `bootstrap.py`.
6. Move `cmd_init` plus the embedded skill/agent template constants into
   `init.py`.
7. Delete root `slices_cli.py`, migrate tests to `import slice_cli`, update type
   checking, packaging config, CI, and contributor docs.

The first PR is the package skeleton (`__init__.py`, `__main__.py`, `cli.py`)
plus the entry-point switch. Later PRs move internals by concern.

## Architecture Decisions

### Flat Repo-Root Layout (no `src/`)

Eng review decision: put the package at `slice_cli/` in the repo root, not
`src/slice_cli/`. A `src/` layout would make `python -m slice_cli` (and any
source-tree run) fail on a fresh clone unless the contributor first runs
`pip install -e .`, because `slice_cli` would not be on `sys.path`. A flat
layout keeps the "works from a bare checkout" guarantee with no `sys.path`
manipulation — explicit over clever.

```
repo/
  slice_cli/
    __init__.py        # explicit package exports (public names only)
    __main__.py        # python -m slice_cli  -> cli.main()
    cli.py             # argparse wiring + main()
    context.py         # Ctx: repo root, slices dir, rel paths, git
    paths.py           # path normalization / resolution helpers
    fingerprint.py     # content + source fingerprinting
    index.py           # index parse/generate, source path discovery
    persistence.py     # load/save slices + doc manifest
    drift.py           # doc drift detection
    check.py           # validation (run_check)
    commands.py        # thin cmd_* handlers
    render.py          # human + JSON rendering (_emit_json, *_human)
    bootstrap.py       # cmd_docs_bootstrap + helpers (~464 lines)
    init.py            # cmd_init + embedded template constants
  test_slices_cli.py   # imports slice_cli
  skills/slice-codebase/SKILL.md   # canonical plugin template (unchanged)
  agents/codebase-slicer.md        # canonical plugin template (unchanged)
```

### Packaged Entry Point

```toml
[project.scripts]
slice = "slice_cli.cli:main"
```

```
Installed user path
  pip/pipx install -> console script: slice = slice_cli.cli:main -> slice_cli/cli.py

Source-tree path (no install)
  python -m slice_cli ... -> slice_cli/__main__.py -> cli.main()
```

`pyproject.toml` ships the package only:

```toml
[tool.setuptools]
packages = ["slice_cli"]
# (no py-modules; root slices_cli.py is deleted)
```

### Dependency Direction

Keep `context.py` low-level. It owns repo-root discovery, slices-dir resolution,
relative path formatting, and git invocation only.

Move `Ctx.source_fingerprint()` out of `Ctx`; source fingerprinting belongs in
`fingerprint.py` / `index.py`. This is what prevents a cycle between
`context.py` and the fingerprint/index helpers.

```
context.py
  <- paths.py / fingerprint.py / persistence.py
  <- drift.py / check.py / commands.py / bootstrap.py / init.py
  <- cli.py
```

No high-level module imports through `context.py` in a way that creates a cycle.

### No Facade — Clean Cut

The root `slices_cli.py` facade is **not** created. Tests and any callers import
`slice_cli` (or its submodules) directly. Tests currently reaching private
helpers (`cli._content_fingerprint`, `cli._resolve_raw_path`, `cli._expand_glob`,
`cli._docs_for_slice`, `cli._normalize_abstraction`, `cli.__file__`,
`cli._SLICE_CODEBASE_SKILL`, `cli._CODEBASE_SLICER_AGENT`) repoint to the real
module that now owns each symbol (e.g. `slice_cli.fingerprint._content_fingerprint`,
`slice_cli.init._SLICE_CODEBASE_SKILL`).

### Templates Stay Embedded (status quo)

Eng review decision: keep templates exactly as they are. The skill/agent
templates remain **embedded Python string constants** (`_SLICE_CODEBASE_SKILL`,
`_CODEBASE_SLICER_AGENT`), which move into `init.py` alongside `cmd_init`. The
canonical on-disk copies stay at repo-root `skills/slice-codebase/SKILL.md` and
`agents/codebase-slicer.md` (read by the plugin). The existing byte-identical
sync test stays — it just repoints its import to `slice_cli.init`.

This means **no `importlib.resources`, no force-include, no packaging change for
templates**: `slice init --agent` already works from an installed wheel because
the constants ship inside the module. (Moving to package-data resources was
considered and rejected to keep the diff small and avoid the source-tree vs
wheel resource-anchor problem.)

### Bootstrap Gets Its Own Module

`cmd_docs_bootstrap` is 464 lines (19% of the current file). It moves into
`bootstrap.py` so `commands.py` stays a scannable list of thin `cmd_*` handlers
and the large, self-contained bootstrap logic is findable by name.

## Implementation Tasks

- [ ] **T1 (P1)** - Packaging - Create `slice_cli/` package skeleton
  (`__init__.py`, `__main__.py`, `cli.py`) and point the console script at
  `slice_cli.cli:main`.
  - Files: `pyproject.toml`, `slice_cli/__init__.py`, `slice_cli/__main__.py`,
    `slice_cli/cli.py`
  - Verify: clean wheel install, then `slice --repo examples/mock-repo list --json`;
    also `python -m slice_cli --repo examples/mock-repo list --json` from a bare
    checkout.

- [ ] **T2 (P1)** - Architecture - Move source fingerprinting out of `Ctx` into
  `fingerprint.py` / `index.py`.
  - Files: `slice_cli/context.py`, `slice_cli/index.py`, `slice_cli/fingerprint.py`
  - Verify: dirty-worktree fingerprint tests still pass; no import cycle.

- [ ] **T3 (P1)** - Clean cut - Delete root `slices_cli.py`; migrate
  `test_slices_cli.py` from `import slices_cli as cli` to `import slice_cli` and
  repoint all private-helper references to their real modules.
  - Files: `slices_cli.py` (delete), `test_slices_cli.py`, `slice_cli/`
  - Verify: `pytest test_slices_cli.py` green; `import slice_cli` works.

- [ ] **T4 (P2)** - Bootstrap + init split - Move `cmd_docs_bootstrap` (+helpers)
  into `bootstrap.py`; move `cmd_init` + embedded template constants into
  `init.py`; repoint the template sync test.
  - Files: `slice_cli/bootstrap.py`, `slice_cli/init.py`, `test_slices_cli.py`
  - Verify: `slice init --agent` tests pass; `test_embedded_templates_match_committed_files`
    passes against `slice_cli.init` constants.

- [ ] **T5 (P1)** - Tests/CI - Add clean wheel-install smoke coverage and
  retarget tooling.
  - Files: `.github/workflows/ci.yml`, `pyproject.toml`, `test_slices_cli.py`
  - Verify: CI builds the wheel, installs it into a clean venv, runs the installed
    `slice` command and `slice init --agent`; pyright retargeted from
    `slices_cli.py` to `slice_cli` + `test_slices_cli.py`; `[tool.coverage.run]`
    source updated to `slice_cli`.

- [ ] **T6 (P2)** - Docs/contributor DX - Update all contributor-facing docs to
  describe the package, and add the navigation map that makes the refactor's value
  real. (DX review: the refactor's entire benefit is contributor navigation, which
  only lands if the docs point each concern to its module.)
  - `CONTRIBUTING.md`: rewrite the "Keep it single-file / don't split into a
    package" convention (it now contradicts the repo) and add the module map below.
  - `CLAUDE.md` (project): rewrite the Layout + "single-file script" convention and
    the `main()`-at-bottom-of-`slices_cli.py` pointer; add the module map.
  - `README.md`: replace any `python slices_cli.py` reference with
    `python -m slice_cli`.
  - `CHANGELOG.md`: add a BREAKING note — module became the `slice_cli` package;
    `import slices_cli` and `python slices_cli.py` removed; use `slice` or
    `python -m slice_cli`.
  - Files: `CONTRIBUTING.md`, `CLAUDE.md`, `README.md`, `CHANGELOG.md`
  - Verify: `grep -rn 'single-file\|slices_cli\.py\|import slices_cli'` returns no
    stale references in docs.

  Contributor module map (add to CONTRIBUTING.md and CLAUDE.md):

  | I want to change...            | Look in          |
  |--------------------------------|------------------|
  | repo / git / path discovery    | `context.py`     |
  | path normalization helpers     | `paths.py`       |
  | content + source fingerprinting| `fingerprint.py` |
  | index parse/generate           | `index.py`       |
  | load/save slices + manifest    | `persistence.py` |
  | doc drift detection            | `drift.py`       |
  | validation (`slice check`)     | `check.py`       |
  | command handlers (`cmd_*`)     | `commands.py`    |
  | human + JSON rendering         | `render.py`      |
  | `slice docs bootstrap`         | `bootstrap.py`   |
  | `slice init` + templates       | `init.py`        |
  | argparse wiring + `main()`     | `cli.py`         |
  | `python -m slice_cli` entry    | `__main__.py`    |

## Test Plan

Run after each stage:

```bash
pytest test_slices_cli.py
pyright slice_cli test_slices_cli.py
python -m build
```

Packaging smoke test requirements:

```text
CODE PATHS                                       USER FLOWS
[+] package skeleton + entry point               [+] User installs slice-cli
  |-- [GAP] build wheel                           |-- [GAP] installed `slice --repo examples/mock-repo list --json`
  |-- [GAP] install wheel in clean venv           |-- [GAP] `python -m slice_cli --repo examples/mock-repo list --json`
  |-- [GAP] console script imports package        `-- [GAP] `slice init --agent` from installed wheel
  `-- [GAP] embedded templates ship in wheel
```

Required smoke checks:

- Build a wheel.
- Install the wheel into a clean virtualenv.
- Run installed `slice --repo examples/mock-repo list --json`.
- Run `python -m slice_cli --repo examples/mock-repo list --json` from a bare
  checkout (no install) — proves the flat layout / source-tree guarantee.
- Run `slice init --agent` from the installed wheel — proves embedded templates
  ship in the wheel.
- `import slice_cli` succeeds.

Obsolete checks to REMOVE (facade dropped): `python slices_cli.py ...` smoke and
the `import slices_cli` facade import test.

Mandatory regression guard (IRON RULE): keep
`test_embedded_templates_match_committed_files`, repointed to
`slice_cli.init._SLICE_CODEBASE_SKILL` / `._CODEBASE_SLICER_AGENT`. It guards the
two-channel template contract (embedded constants vs canonical on-disk files).

Prior learning: commands against `examples/mock-repo` must pass
`--repo examples/mock-repo`; do not rely on `cd examples/mock-repo` because repo
discovery climbs to the parent git repo.

## What Already Exists

- `Ctx`, dataclasses, persistence, drift, validation, command handlers, parser,
  bootstrap, and init behavior already exist in `slices_cli.py`; this is a
  move/refactor, not a rewrite.
- `test_slices_cli.py` (121 tests) already covers core CLI behavior, doc drift,
  dirty-tree fingerprinting, verification links, init, bootstrap, and help text.
  The suite IS the regression net for the import migration.
- `slice init --agent` already works from an installed wheel today (embedded
  constants ship in the module) — no packaging work needed for that to continue.
- `.github/workflows/ci.yml` already runs pytest across Python 3.10 to 3.13 and
  pyright (currently targeting `slices_cli.py` — T5 retargets it).
- `skills/slice-codebase/SKILL.md` and `agents/codebase-slicer.md` are the
  canonical on-disk template files; the byte-identical sync test already exists.

## NOT in Scope

- New CLI features or command behavior changes.
- Rewriting command semantics into a full domain-core/adapters architecture.
- Changing `slices_cli_upstream.py` (read-only reference).
- Moving templates to `importlib.resources` / package-data (considered, rejected
  — embedded constants already solve wheel distribution).
- Preserving `import slices_cli` or `python slices_cli.py` (intentionally
  dropped).
- New publishing channels beyond the existing Python package path.
- `src/` layout (rejected in favor of flat repo-root layout).

## Failure Modes

- Broken installed console script: cover with clean wheel smoke test (T5).
- `python -m slice_cli` fails on a bare clone: prevented by flat layout; covered
  by the no-install smoke check.
- Circular import between `context.py` and fingerprint/index helpers: prevented by
  moving source fingerprinting out of `Ctx` (T2).
- Test import migration silently points at the wrong module: the 121-test suite
  is the net; a mis-repointed private helper fails its test.
- Embedded templates drift from canonical on-disk files: covered by the retained
  `test_embedded_templates_match_committed_files` (T4).
- Stale docs after the cut: `CLAUDE.md` and `README.md` reference `slices_cli.py`
  as "the version we develop against" and guarantee `python slices_cli.py` —
  these become wrong the moment the file is deleted. T6 fixes them; until then
  this is a known doc-staleness gap (flagged, not silent).

## Parallelization

| Step | Modules touched | Depends on |
|------|-----------------|------------|
| Package skeleton + entry point | packaging, cli, __main__ | none |
| Context/fingerprint split | context, paths, fingerprint, index | package skeleton |
| Persistence + drift/check extraction | persistence, drift, check | context/fingerprint split |
| Commands/render extraction | commands, render, cli | drift/check extraction |
| Bootstrap extraction | bootstrap | package skeleton |
| Init + templates | init, tests | package skeleton |
| Clean cut + docs/CI | slices_cli.py (delete), tests, CI, pyproject, CLAUDE.md, README.md | all of the above |

Recommended lanes:

- Lane A: package skeleton -> context/fingerprint -> persistence/drift/check ->
  commands/render
- Lane B: bootstrap extraction after package skeleton
- Lane C: init + templates after package skeleton
- Lane D: clean cut + docs/CI last (depends on everything; deletes the old file)

Conflict flags: `pyproject.toml` and `test_slices_cli.py` are shared by most
lanes — prefer sequential merges for those files. The clean-cut lane must merge
last because it deletes `slices_cli.py`.

## Developer Experience Review

DX lens: end-user (`slice` CLI) + contributor (codebase navigation). The refactor
holds end-user behavior stable, so the DX value lives in contributor onboarding.

Persona (contributor): OSS contributor / maintainer landing in the repo to make a
change. Win condition: find the file that owns a concern in under 2 minutes without
reading the whole codebase.

End-user journey: `pip install slice-cli` -> `slice` works. README drives everything
through `slice` (not `python slices_cli.py`), so dropping that path costs documented
users nothing. End-user TTHW unchanged (Champion, <2 min) provided the T5 wheel smoke
test passes.

Contributor friction found (all fixed in T6):
1. `CONTRIBUTING.md` told contributors "keep it single-file, don't split into a
   package" — the refactor contradicts the first doc a contributor reads. (Credible)
2. The navigation payoff was invisible without a concern->module map. (Findable)
3. The breaking change to the public import/run surface was unrecorded. (Credible)

DX scorecard: Getting Started 9/10 (unchanged end-user flow). Contributor
navigation 6/10 -> 9/10 after the T6 map + doc fixes. Upgrade path 6/10 -> 9/10
after the CHANGELOG note. Overall 7/10 -> 9/10.

## Review Notes

- Eng review (2026-05-29) reshaped the plan:
  - Module granularity: full ~11-module split accepted as-is, plus `bootstrap.py`.
  - Layout: flat repo-root `slice_cli/`, not `src/` (source-tree import safety).
  - Facade: dropped entirely; tests migrate to `import slice_cli`;
    `python -m slice_cli` replaces `python slices_cli.py`.
  - Bootstrap: its own module.
  - Templates: kept embedded (status quo); T4 (resources migration) dropped.
- Architecture issues found: 3 (all resolved by the decisions above).
- Code quality issues found: 1 (template DRY — user chose to keep status quo).
- Test review: coverage diagram produced, 5 packaging gaps folded into T5/T1,
  2 obsolete checks removed, 1 regression guard retained.
- Performance review: no issues (behavior-preserving refactor).
- Outside voice: skipped (devex review queued as the next lens).
- Critical gaps: 0.

## GSTACK REVIEW REPORT

| Review | Trigger | Why | Runs | Status | Findings |
|--------|---------|-----|------|--------|----------|
| CEO Review | `/plan-ceo-review` | Scope & strategy | 0 | — | — |
| Codex Review | `/codex review` | Independent 2nd opinion | 0 | — | — |
| Eng Review | `/plan-eng-review` | Architecture & tests (required) | 1 | CLEAR (PLAN) | 4 issues, 0 critical gaps |
| Design Review | `/plan-design-review` | UI/UX gaps | 0 | n/a (CLI) | — |
| DX Review | `/plan-devex-review` | Developer experience gaps | 1 | CLEAR | score 7→9, 3 contributor-DX fixes (all in T6) |

- **UNRESOLVED:** 0
- **VERDICT:** ENG + DX CLEARED — ready to implement.
