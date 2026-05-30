---
doc_id: rust-test-parity-gap-map
title: Rust Test Parity Gap Map
tags: [plan, rust, testing, parity]
---

# Rust Test Parity Gap Map

Status: stage-3 step 1 complete (gap-map) — hand-off work doc for the backfill
Generated: 2026-05-30

## Purpose

Stage 3 of the [Rust-forward migration](rust-gap-closure-plan.md) backfills the
Rust test suite to match the 118-test Python oracle, as **Python-independent
snapshots** so Python can be deleted in Stage 4. This doc is the grounded
inventory: every Python test class, what Rust already exercises, and the precise
gaps.

## How to read the counts

The Python suite has **118 tests across 19 classes**. The Rust suite has **16
integration parity tests + 6 unit tests**. The numbers are not comparable
1:1 — each Rust parity test loops over many cases (e.g. `read_only_json_matches_python`
covers 16 command invocations in one test). So Rust already covers ~60 Python
tests *behaviorally*. What's missing is mostly **edge cases, error paths, and
unit-level helpers**.

## End-state principle (read this first)

The Python package, the `python3 -m slice_cli` comparison in `parity.rs`, the
`pytest` suite, and `slices_cli_upstream.py` are **all throwaway scaffolding**.
The end state is a pure-Rust repo whose tests assert on **known-correct values
baked into the Rust tests** — no Python at runtime, no differential comparison,
no oracle.

Python is useful *only transiently*: it's the convenient way to derive each
expected value while authoring a test (run the Python command once, read its
output, hardcode that as the Rust test's expectation). Once a behavior has a
native Rust test, its differential counterpart is dead weight.

So the two workstreams are:

- **(A) Behavioral gaps** — author ~56 missing scenarios as native Rust tests.
- **(B) De-Python the existing tests** — rewrite the 16 differential tests to
  assert on baked-in expected values instead of shelling out to Python.

When (A) and (B) are done, Stage 4 deletes everything Python in one cut.

## Coverage by class

Legend: ✅ behaviorally covered · 🟡 partial (command exercised, this edge not) · ❌ gap

| Class | # | Covered | Gaps to author |
|-------|---|---------|----------------|
| TestGlobExpansion | 3 | ✅ 3 (paths.rs unit tests) | — |
| TestContextConfig | 4 | ✅ 4 (`context_config_*`) | — |
| TestDocsBootstrap | 2 | ✅ 2 (`native_docs_bootstrap_*`) | — |
| TestShowSections | 7 | ✅ 6 | ❌ missing_section_does_not_fail |
| TestCLI | 10 | ✅ 8 | 🟡 unknown_slice error, show_includes_manifest_docs |
| TestContext | 5 | ✅ 3 | ❌ no_owner_fails · 🟡 missing_sections_ok |
| TestCheck | 11 | ✅ 7 | ❌ dirty-source staleness, dirty-worktree staleness, drift-warning (staleness ON), index_fingerprint_stable |
| TestInit | 13 | ✅ 8 | ❌ **embedded_templates_match_committed_files (IRON RULE)**, preserves_existing_claudemd, updates_agents_md, without_agent_skips_skill, help_examples |
| TestCommandCoverage | 7 | ✅ 4 | ❌ deps_transitive, deps_transitive_handles_cycle, grep_without_rg_is_graceful |
| TestVerificationLinks | 8 | ✅ 4 | ❌ parse_extracts, parse_ignores_freetext, normalize_abstraction, symbol_part_not_validated (unit) |
| TestAffectedDocs | 5 | ✅ 1 | ❌ when_current vs when_stale, unknown_file_empty, text_output, multiple_paths |
| TestStamp | 12 | ✅ 5 | ❌ by_path, by_slice, all_stale, dirty_tree_allowed, rebase_after_stamp, legacy_sha_fallback, no_manifest |
| TestDocDrift | 9 | ✅ 3 | ❌ drift_after_source_change, reports_affected_slices, missing_verified_at, bad_sha_reports_error, detects_uncommitted_changes, multi_slice_doc |
| TestManifestLoading | 6 | 🟡 indirect | ❌ load, fields, vault_root, no_manifest_empty, reverse_lookup ×2 (unit) |
| TestCtx | 3 | 🟡 indirect | ❌ head_sha, docs_manifest_path, rel (unit) |
| TestSectionExtraction | 3 | 🟡 indirect | ❌ parses_h2, ignores_h3, empty_without_headings (unit) |
| TestRobustness | 5 | ❌ 0 | ❌ malformed_docs_yaml exit 2, malformed_slice_frontmatter exit 2, not_a_git_repo exit 2, env_var_repo_root, stale_docs_help exit-code docs |
| TestIncludeExclude | 2 | ❌ 0 | ❌ include_narrows_scope, exclude_filters_paths |
| TestContextHelp | 3 | ❌ 0 | ❌ help_advertises_context, context_help_examples, show_help_section_flags |
| **Total** | **118** | **~62** | **~56** |

## Gaps grouped by priority

### P1 — user-visible behavior / correctness (do first)
- **Error & exit-code paths (Robustness, 5):** malformed YAML → exit 2, malformed
  frontmatter → exit 2, not-a-git-repo → exit 2, `SLICE_REPO` env var, exit-code
  help text. Currently *zero* Rust coverage of the failure surface.
- **Stamp edge cases (7):** by_path, by_slice, all-stale, dirty-tree-allowed,
  rebase-after-stamp-not-stale, legacy-SHA fallback, no-manifest.
- **Doc drift edge cases (6):** source-change drift, affected-slice reporting,
  missing `verified_at`, bad SHA, uncommitted/dirty-worktree drift, multi-slice doc.
- **Check staleness-ON paths (4):** the existing Rust check tests almost all pass
  `--no-staleness`; the dirty-source / dirty-worktree / drift-warning paths with
  staleness enabled are under-tested.
- **Init (5):** `embedded_templates_match_committed_files` is the **IRON RULE**
  two-channel guard (embedded Rust templates vs committed `SKILL.md`/agent) — must
  port. Plus preserves-existing-CLAUDE.md, updates-AGENTS.md, without-agent, help.
- **affected-docs (4):** current-vs-stale, unknown-file-empty, human text, multi-path.

### P2 — coverage completeness
- include/exclude filtering (2); deps transitive + cycle, grep-without-rg (3);
  CLI unknown-slice + show-manifest-docs (2); context no-owner + missing-sections
  (2); show missing-section (1).

### P3 — unit-level helpers (often satisfied by native Rust unit tests)
- ManifestLoading (6), Ctx (3), SectionExtraction (3), VerificationLinks parsing
  (4) — these test internal Python functions; in Rust they become module unit
  tests in `manifest.rs` / `context.rs` / `slices.rs`. Context help text (3) is
  low value (human help strings) — candidate for an explicit "intentionally not
  ported" note rather than a test.

## Mechanism

No snapshot framework is required. Native Rust assertions on baked-in expected
values are enough and keep the dep list minimal:

- **JSON outputs** — assert on a `serde_json::json!({...})` literal (or a small
  committed `.json` fixture for large outputs). Replaces `assert_eq!(rust, python)`
  with `assert_eq!(rust, expected_literal)`.
- **Human/text outputs** — assert on an inline expected string, or a committed
  `.txt` fixture for multi-line output.
- **Exit codes & stderr** — assert on the literal code and a `contains(...)` on the
  message.

`insta` is optional sugar if bulk snapshot review becomes painful; not needed to
start, and avoidable entirely given the end goal is zero external test deps.

Keep the temp-repo `fixture_repo()` helper in `parity.rs` (or move it to a shared
test module) — building throwaway git repos is still how state-changing commands
are tested; only the *Python comparison* goes away.

## Work checklist

Each `[ ]` is a natural sub-commit. Derive expected values by running the Python
command once (`python -m slice_cli --repo <r> <cmd>`), then bake them in.

**(B) De-Python existing tests** (do first — sets the native-assertion pattern):
- [ ] `read_only_json_matches_python` → `read_only_json` (16 invocations, JSON literals)
- [ ] `read_only_human_outputs_match_python` → human-string assertions
- [ ] `subprocess_commands_and_init_dry_runs_match_python`
- [ ] `affected_docs_*`, `native_write_*`, `native_docs_bootstrap_*`
- [ ] the 7 `native_check_json_*` tests
- [ ] `context_config_and_ambiguity_*`, `native_init_*`, `fingerprint_staleness_*`
- [ ] rename the file off `parity.rs` (it's no longer parity) — e.g. `cli.rs` / split per command

**(A) P1 behavioral gaps** (one sub-commit per group):
- [ ] Robustness/errors (5): malformed `DOCS.yaml` → exit 2; malformed slice
      frontmatter → exit 2; not-a-git-repo → exit 2; `SLICE_REPO` env var honored;
      `stale-docs -h` documents exit codes
- [ ] Stamp (7): by_path, by_slice, all-stale, dirty-tree-allowed,
      rebase-after-stamp-not-stale, legacy-SHA fallback flags drift, no-manifest
- [ ] Doc drift (6): source-change drift, affected-slice reporting, missing
      `verified_at` always stale, bad SHA error, uncommitted/dirty drift, multi-slice doc
- [ ] Check staleness-ON (4): dirty-source fingerprint, dirty-worktree staleness,
      doc-drift-as-warning (staleness enabled), index fingerprint stable across dirty→commit
- [ ] Init (5): **`embedded_templates_match_committed_files` (IRON RULE)** — assert
      the embedded Rust template constants are byte-identical to `skills/slice-codebase/SKILL.md`
      and `agents/codebase-slicer.md`; preserves-existing-CLAUDE.md; updates-AGENTS.md;
      without-`--agent`-skips-skill; help examples
- [ ] affected-docs (4): when-current vs when-stale, unknown-file-empty, human text, multi-path

**(A) P2 completeness:**
- [ ] include/exclude filtering (2); deps transitive + cycle (2); grep-without-rg
      graceful (1); CLI unknown-slice error + show-manifest-docs (2); context
      no-owner-fails + missing-sections-ok (2); show missing-section (1)

**(A) P3 unit-level** (native module `#[cfg(test)]` in the owning Rust module):
- [ ] manifest.rs: load, fields, vault_root, no-manifest-empty, reverse_lookup ×2
- [ ] context.rs: head_sha, docs_manifest_path, rel
- [ ] slices.rs: section extraction parses-h2, ignores-h3, empty-without-headings
- [ ] verification parsing: extracts links+upstream, ignores freetext, normalize
      abstraction, symbol-part-not-validated
- [ ] context help text (3): port **or** add a one-line "intentionally not ported"
      note to this doc

**Reconcile + Stage 4:**
- [ ] Confirm every one of the 118 Python tests maps to a Rust test or a documented
      non-port (table above, all rows resolved)
- [ ] Delete `slice_cli/`, `test_slices_cli.py`, `slices_cli_upstream.py`
- [ ] Strip Python from `pyproject.toml` (or delete it), the `python-oracle` CI job,
      and `pip install` steps in the `rust` CI job
- [ ] Remove the `python3` helpers from the test file; final docs pass
      (CLAUDE.md/CONTRIBUTING/README drop all oracle references)
- [ ] Python remains recoverable via tag `python-impl-final` + branch `package-refactor`
