---
doc_id: rust-test-parity-gap-map
title: Rust Test Parity Gap Map
tags: [plan, rust, testing, parity]
---

# Rust Test Parity Gap Map

Status: stage-3 step 1 (gap-map) — drives the coverage backfill
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

Two distinct workstreams fall out of this:

- **(A) Behavioral gaps** — ~56 Python scenarios no Rust test currently exercises.
- **(B) Snapshot conversion** — all 16 existing Rust tests are *differential*
  (they shell out to `python3 -m slice_cli` and compare). Every one must be
  re-expressed as a committed snapshot before Python can leave the tree.

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

## Cross-cutting: differential → snapshot conversion

The snapshot end-state means Python cannot be a runtime test dependency. Plan:

1. Pick the snapshot mechanism (likely `insta`, or plain committed `.json`/`.txt`
   fixtures matching the existing assert-on-JSON style).
2. For each case (existing + new), run the Python oracle **once** to capture the
   expected output, commit it as a fixture, and assert the Rust binary against the
   fixture — no `Command::new("python3")` at test time.
3. Keep the differential tests alongside the snapshots until Stage 4, then delete
   the differential layer (it's what pins Python in the tree).

## Recommended execution order (sub-commits)

1. Snapshot harness + convert the 16 existing differential tests (no new behavior,
   just Python-independence) — proves the mechanism before scaling.
2. P1 group, by command: robustness/errors → stamp → doc-drift → check-staleness →
   init → affected-docs. One sub-commit per command group.
3. P2 completeness pass.
4. P3 unit tests (mostly authored as native Rust module tests).
5. Coverage reconciliation: confirm every Python test maps to a Rust test or a
   documented intentional non-port; then Stage 4 deletes Python.
