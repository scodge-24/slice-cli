"""Tests for slice_cli — manifest-based doc tracking."""

from __future__ import annotations

import json
import subprocess
import textwrap
from pathlib import Path

import pytest
import yaml

import slice_cli as cli
from slice_cli import commands as commands_mod
from slice_cli import drift, fingerprint, init as init_mod, paths, render


# ---------------------------------------------------------------------------
# Fixtures
# ---------------------------------------------------------------------------

@pytest.fixture()
def repo(tmp_path: Path) -> Path:
    """Minimal git repo with source files, docs, slices, and DOCS.yaml."""
    subprocess.run(["git", "init"], cwd=tmp_path, capture_output=True, check=True)
    subprocess.run(["git", "config", "user.email", "t@t"], cwd=tmp_path, capture_output=True, check=True)
    subprocess.run(["git", "config", "user.name", "t"], cwd=tmp_path, capture_output=True, check=True)

    # Source files
    for d in ("src/auth", "src/api", "src/models"):
        (tmp_path / d).mkdir(parents=True)
    (tmp_path / "src/auth/middleware.py").write_text("def verify_token(): pass\n")
    (tmp_path / "src/auth/sessions.py").write_text("def create_session(): pass\n")
    (tmp_path / "src/api/handlers.py").write_text("def get_user(): pass\n")
    (tmp_path / "src/api/routes.py").write_text("ROUTES = {}\n")
    (tmp_path / "src/models/user.py").write_text("class User: pass\n")

    # Docs (with doc_id frontmatter)
    docs = tmp_path / "docs"
    docs.mkdir()
    (docs / "auth-guide.md").write_text(
        "---\ndoc_id: auth-guide\ntitle: Auth Guide\n---\n# Auth Guide\nUse verify_token.\n"
    )
    (docs / "api-ref.md").write_text(
        "---\ndoc_id: api-ref\ntitle: API Reference\n---\n# API Reference\n"
    )
    (docs / "data-model.md").write_text(
        "---\ndoc_id: data-model\ntitle: Data Model\n---\n# Data Model\n"
    )

    # Initial commit
    subprocess.run(["git", "add", "-A"], cwd=tmp_path, capture_output=True, check=True)
    subprocess.run(["git", "commit", "-m", "initial"], cwd=tmp_path, capture_output=True, check=True)
    base_sha = subprocess.run(
        ["git", "rev-parse", "HEAD"],
        cwd=tmp_path, capture_output=True, text=True, check=True,
    ).stdout.strip()

    # Slices (no docs: field)
    slices = tmp_path / "slices"
    slices.mkdir()

    (slices / "auth-service.md").write_text(textwrap.dedent("""\
        ---
        slice_id: auth-service
        description: Auth and sessions
        loc: 30
        files:
          - src/auth/middleware.py
          - src/auth/sessions.py
        dependencies: []
        ---
        Auth slice body.
    """))

    (slices / "api-handlers.md").write_text(textwrap.dedent("""\
        ---
        slice_id: api-handlers
        description: API handlers and routing
        loc: 20
        files:
          - src/api/handlers.py
          - src/api/routes.py
        dependencies:
          - auth-service
        ---
        API slice body.
    """))

    (slices / "data-model.md").write_text(textwrap.dedent("""\
        ---
        slice_id: data-model
        description: Core data models
        loc: 10
        files:
          - src/models/user.py
        dependencies: []
        ---
        Data model body.
    """))

    # INDEX.md
    (slices / "INDEX.md").write_text(textwrap.dedent(f"""\
        # Slice Index

        Last updated: {base_sha}

        | Slice ID | Description | LoC |
        |----------|-------------|-----|
        | `api-handlers` | API handlers and routing | ~20 |
        | `auth-service` | Auth and sessions | ~30 |
        | `data-model` | Core data models | ~10 |
    """))

    # DOCS.yaml manifest — keyed by doc_id, vault_root relative to slices/
    (slices / "DOCS.yaml").write_text(yaml.dump({
        "vault_root": "../docs",
        "docs": {
            "auth-guide": {
                "path": "auth-guide.md",
                "slices": ["auth-service"],
                "verified_at": base_sha[:12],
                "tags": ["auth", "middleware"],
            },
            "api-ref": {
                "path": "api-ref.md",
                "slices": ["api-handlers"],
                "verified_at": base_sha[:12],
                "tags": ["api", "routes"],
            },
            "data-model": {
                "path": "data-model.md",
                "slices": ["data-model"],
                "verified_at": base_sha[:12],
                "tags": ["models"],
            },
        },
    }, default_flow_style=False, sort_keys=False))

    # Commit slices + manifest
    subprocess.run(["git", "add", "-A"], cwd=tmp_path, capture_output=True, check=True)
    subprocess.run(["git", "commit", "-m", "add slices"], cwd=tmp_path, capture_output=True, check=True)

    return tmp_path


@pytest.fixture()
def ctx(repo: Path) -> cli.Ctx:
    return cli.Ctx(repo=str(repo))


def _make_drift(repo: Path) -> str:
    """Modify a source file and commit, returning the new HEAD SHA."""
    (repo / "src" / "auth" / "middleware.py").write_text("def verify_token():\n    raise NotImplementedError\n")
    subprocess.run(["git", "add", "-A"], cwd=repo, capture_output=True, check=True)
    subprocess.run(["git", "commit", "-m", "break middleware"], cwd=repo, capture_output=True, check=True)
    return subprocess.run(
        ["git", "rev-parse", "HEAD"], cwd=repo, capture_output=True, text=True, check=True,
    ).stdout.strip()


# ---------------------------------------------------------------------------
# Manifest loading
# ---------------------------------------------------------------------------

class TestManifestLoading:
    def test_load_manifest(self, ctx: cli.Ctx):
        manifest = cli.load_doc_manifest(ctx)
        assert len(manifest.docs) == 3
        ids = {td.doc_id for td in manifest.docs}
        assert "auth-guide" in ids

    def test_manifest_fields(self, ctx: cli.Ctx):
        manifest = cli.load_doc_manifest(ctx)
        auth = next(td for td in manifest.docs if td.doc_id == "auth-guide")
        assert auth.path == "auth-guide.md"
        assert auth.slices == ("auth-service",)
        assert auth.verified_at  # non-empty
        assert "auth" in auth.tags

    def test_vault_root_resolved(self, ctx: cli.Ctx):
        manifest = cli.load_doc_manifest(ctx)
        vr = manifest.vault_root(ctx)
        assert vr is not None
        assert (vr / "auth-guide.md").exists()

    def test_no_manifest_returns_empty(self, ctx: cli.Ctx):
        ctx.docs_manifest_path.unlink()
        manifest = cli.load_doc_manifest(ctx)
        assert manifest.docs == []

    def test_reverse_lookup(self, ctx: cli.Ctx):
        manifest = cli.load_doc_manifest(ctx)
        auth_docs = drift._docs_for_slice(manifest.docs, "auth-service")
        assert len(auth_docs) == 1
        assert auth_docs[0].doc_id == "auth-guide"

    def test_reverse_lookup_no_match(self, ctx: cli.Ctx):
        manifest = cli.load_doc_manifest(ctx)
        assert drift._docs_for_slice(manifest.docs, "nonexistent") == []


# ---------------------------------------------------------------------------
# Doc drift (manifest-based)
# ---------------------------------------------------------------------------

def _write_manifest(ctx: cli.Ctx, docs: dict, vault_root: str = "../docs") -> None:
    """Helper: write a DOCS.yaml with doc_id keying."""
    ctx.docs_manifest_path.write_text(
        yaml.dump({"vault_root": vault_root, "docs": docs}, default_flow_style=False),
        encoding="utf-8",
    )


class TestDocDrift:
    def test_no_drift_when_clean(self, ctx: cli.Ctx):
        manifest = cli.load_doc_manifest(ctx)
        slices = cli.load_slice_docs(ctx)
        assert cli.check_doc_drift(manifest.docs, slices, ctx) == []

    def test_drift_after_source_change(self, repo: Path, ctx: cli.Ctx):
        _make_drift(repo)
        manifest = cli.load_doc_manifest(ctx)
        slices = cli.load_slice_docs(ctx)
        drifted = cli.check_doc_drift(manifest.docs, slices, ctx)
        ids = {d.doc_id for d in drifted}
        assert "auth-guide" in ids
        assert "api-ref" not in ids
        assert "data-model" not in ids

    def test_drift_reports_affected_slices(self, repo: Path, ctx: cli.Ctx):
        _make_drift(repo)
        manifest = cli.load_doc_manifest(ctx)
        slices = cli.load_slice_docs(ctx)
        drifted = cli.check_doc_drift(manifest.docs, slices, ctx)
        auth_drift = next(d for d in drifted if d.doc_id == "auth-guide")
        assert "auth-service" in auth_drift.affected_slices

    def test_drift_reports_changed_files(self, repo: Path, ctx: cli.Ctx):
        _make_drift(repo)
        manifest = cli.load_doc_manifest(ctx)
        slices = cli.load_slice_docs(ctx)
        drifted = cli.check_doc_drift(manifest.docs, slices, ctx)
        auth_drift = next(d for d in drifted if d.doc_id == "auth-guide")
        assert "src/auth/middleware.py" in auth_drift.changed_files

    def test_missing_verified_at_always_stale(self, ctx: cli.Ctx):
        _write_manifest(ctx, {
            "auth-guide": {"path": "auth-guide.md", "slices": ["auth-service"], "verified_at": ""},
        })
        manifest = cli.load_doc_manifest(ctx)
        slices = cli.load_slice_docs(ctx)
        drifted = cli.check_doc_drift(manifest.docs, slices, ctx)
        assert len(drifted) == 1
        assert drifted[0].verified_at == "(never)"

    def test_bad_sha_reports_error(self, ctx: cli.Ctx):
        _write_manifest(ctx, {
            "auth-guide": {"path": "auth-guide.md", "slices": ["auth-service"], "verified_at": "deadbeef0000deadbeef"},
        })
        manifest = cli.load_doc_manifest(ctx)
        slices = cli.load_slice_docs(ctx)
        drifted = cli.check_doc_drift(manifest.docs, slices, ctx)
        assert len(drifted) == 1
        assert "git error" in drifted[0].changed_files[0]

    def test_drift_detects_uncommitted_changes(self, repo: Path, ctx: cli.Ctx):
        (repo / "src" / "auth" / "middleware.py").write_text("# uncommitted change\n")
        manifest = cli.load_doc_manifest(ctx)
        slices = cli.load_slice_docs(ctx)
        drifted = cli.check_doc_drift(manifest.docs, slices, ctx)
        assert any(d.doc_id == "auth-guide" for d in drifted)

    def test_multi_slice_doc(self, ctx: cli.Ctx):
        """Doc spanning multiple slices detects drift in any of them."""
        _write_manifest(ctx, {
            "auth-guide": {
                "path": "auth-guide.md",
                "slices": ["auth-service", "api-handlers"],
                "verified_at": ctx.head_sha()[:12],
            },
        })
        # Modify a file only in api-handlers
        (ctx.repo_root / "src" / "api" / "handlers.py").write_text("# changed\n")
        manifest = cli.load_doc_manifest(ctx)
        slices = cli.load_slice_docs(ctx)
        drifted = cli.check_doc_drift(manifest.docs, slices, ctx)
        assert len(drifted) == 1
        assert "api-handlers" in drifted[0].affected_slices

    def test_fingerprint_drift_narrows_changed_files(self, repo: Path, ctx: cli.Ctx):
        # A fingerprinted doc that drifts reports only the files that actually
        # changed — not its whole tracked set (matching the legacy SHA branch).
        assert cli.main(["--repo", str(repo), "stamp", "auth-guide"]) == 0
        (repo / "src" / "auth" / "middleware.py").write_text("def verify_token():\n    return 0\n")
        subprocess.run(["git", "add", "-A"], cwd=repo, capture_output=True, check=True)
        subprocess.run(["git", "commit", "-m", "edit middleware"], cwd=repo, capture_output=True, check=True)
        manifest = cli.load_doc_manifest(ctx)
        slices = cli.load_slice_docs(ctx)
        drift = next(d for d in cli.check_doc_drift(manifest.docs, slices, ctx)
                     if d.doc_id == "auth-guide")
        assert "src/auth/middleware.py" in drift.changed_files
        assert "src/auth/sessions.py" not in drift.changed_files


# ---------------------------------------------------------------------------
# Glob expansion
# ---------------------------------------------------------------------------

class TestGlobExpansion:
    def test_glob_expands_to_matching_files(self, repo: Path):
        names = {p.name for p in paths._expand_glob("src/auth/*.py", repo)}
        assert names == {"middleware.py", "sessions.py"}

    def test_literal_filename_with_metachars_resolves(self, repo: Path):
        # A real file whose name contains glob metacharacters (e.g. a Next.js
        # route file) must resolve to itself, not be dropped as a non-matching glob.
        route = repo / "app" / "[id]"
        route.mkdir(parents=True)
        target = route / "page.tsx"
        target.write_text("export default Page\n")
        assert paths._expand_glob("app/[id]/page.tsx", repo) == [target.resolve()]

    def test_metachar_file_edits_are_fingerprinted(self, repo: Path, ctx: cli.Ctx):
        route = repo / "app" / "[id]"
        route.mkdir(parents=True)
        (route / "page.tsx").write_text("v1\n")
        fp1 = fingerprint._content_fingerprint(
            sorted(paths._resolve_raw_path("app/[id]/page.tsx", ctx)), ctx.repo_root)
        (route / "page.tsx").write_text("v2\n")
        fp2 = fingerprint._content_fingerprint(
            sorted(paths._resolve_raw_path("app/[id]/page.tsx", ctx)), ctx.repo_root)
        assert fp1 != fp2  # the edit is visible to staleness tracking


# ---------------------------------------------------------------------------
# Include/exclude
# ---------------------------------------------------------------------------

class TestIncludeExclude:
    def test_include_narrows_scope(self, ctx: cli.Ctx):
        """Include overrides slice files — only check listed paths."""
        _write_manifest(ctx, {
            "auth-guide": {
                "path": "auth-guide.md",
                "slices": ["auth-service"],
                "verified_at": ctx.head_sha()[:12],
                "include": ["src/auth/sessions.py"],  # only this file
            },
        })
        # Modify middleware (NOT in include) — should not trigger drift
        (ctx.repo_root / "src" / "auth" / "middleware.py").write_text("# changed\n")
        manifest = cli.load_doc_manifest(ctx)
        slices = cli.load_slice_docs(ctx)
        drifted = cli.check_doc_drift(manifest.docs, slices, ctx)
        assert len(drifted) == 0

    def test_exclude_filters_paths(self, ctx: cli.Ctx):
        """Exclude removes specific files from consideration."""
        _write_manifest(ctx, {
            "auth-guide": {
                "path": "auth-guide.md",
                "slices": ["auth-service"],
                "verified_at": ctx.head_sha()[:12],
                "exclude": ["src/auth/middleware.py"],
            },
        })
        # Modify middleware (excluded) — should not trigger
        (ctx.repo_root / "src" / "auth" / "middleware.py").write_text("# changed\n")
        manifest = cli.load_doc_manifest(ctx)
        slices = cli.load_slice_docs(ctx)
        drifted = cli.check_doc_drift(manifest.docs, slices, ctx)
        assert len(drifted) == 0


# ---------------------------------------------------------------------------
# Stamp (writes to manifest)
# ---------------------------------------------------------------------------

class TestStamp:
    def test_stamp_by_doc_id(self, repo: Path, ctx: cli.Ctx):
        new_sha = _make_drift(repo)
        rc = cli.main(["--repo", str(repo), "stamp", "auth-guide"])
        assert rc == 0
        manifest = cli.load_doc_manifest(ctx)
        auth = next(td for td in manifest.docs if td.doc_id == "auth-guide")
        assert auth.verified_at == new_sha[:12]

    def test_stamp_by_path(self, repo: Path, ctx: cli.Ctx):
        new_sha = _make_drift(repo)
        rc = cli.main(["--repo", str(repo), "stamp", "--doc", "auth-guide.md"])
        assert rc == 0
        manifest = cli.load_doc_manifest(ctx)
        auth = next(td for td in manifest.docs if td.doc_id == "auth-guide")
        assert auth.verified_at == new_sha[:12]

    def test_stamp_by_slice(self, repo: Path, ctx: cli.Ctx):
        _make_drift(repo)
        rc = cli.main(["--repo", str(repo), "stamp", "--slice", "auth-service"])
        assert rc == 0
        manifest = cli.load_doc_manifest(ctx)
        auth = next(td for td in manifest.docs if td.doc_id == "auth-guide")
        assert auth.verified_at != ""  # Updated

    def test_stamp_all_stale(self, repo: Path, ctx: cli.Ctx):
        _make_drift(repo)
        rc = cli.main(["--repo", str(repo), "stamp"])
        assert rc == 0
        # Verify all clean now
        manifest = cli.load_doc_manifest(ctx)
        slices = cli.load_slice_docs(ctx)
        assert cli.check_doc_drift(manifest.docs, slices, ctx) == []

    def test_stamp_preserves_tags(self, repo: Path, ctx: cli.Ctx):
        _make_drift(repo)
        cli.main(["--repo", str(repo), "stamp", "auth-guide"])
        manifest = cli.load_doc_manifest(ctx)
        auth = next(td for td in manifest.docs if td.doc_id == "auth-guide")
        assert "auth" in auth.tags

    def test_stamp_records_fingerprint(self, repo: Path, ctx: cli.Ctx):
        cli.main(["--repo", str(repo), "stamp", "auth-guide"])
        manifest = cli.load_doc_manifest(ctx)
        auth = next(td for td in manifest.docs if td.doc_id == "auth-guide")
        assert len(auth.fingerprint) == 64  # sha256 hex

    def test_stamp_dirty_tree_now_allowed(self, repo: Path, ctx: cli.Ctx):
        # The dirty-guard is gone: stamping uncommitted content is correct and
        # records a fingerprint of exactly that content.
        (repo / "src" / "auth" / "middleware.py").write_text("def verify_token():\n    return None\n")
        rc = cli.main(["--repo", str(repo), "stamp", "auth-guide"])
        assert rc == 0
        manifest = cli.load_doc_manifest(ctx)
        slices = cli.load_slice_docs(ctx)
        drift = cli.check_doc_drift(manifest.docs, slices, ctx)
        assert all(d.doc_id != "auth-guide" for d in drift)

    def test_edit_stamp_commit_not_stale(self, repo: Path, ctx: cli.Ctx):
        # REGRESSION: the exact sequencing the git-SHA anchor broke.
        # Edit a tracked source, stamp while dirty, then commit -> NOT stale.
        (repo / "src" / "auth" / "middleware.py").write_text("def verify_token():\n    return True\n")
        assert cli.main(["--repo", str(repo), "stamp", "auth-guide"]) == 0
        subprocess.run(["git", "add", "-A"], cwd=repo, capture_output=True, check=True)
        subprocess.run(["git", "commit", "-m", "edit auth"], cwd=repo, capture_output=True, check=True)
        manifest = cli.load_doc_manifest(ctx)
        slices = cli.load_slice_docs(ctx)
        drift = cli.check_doc_drift(manifest.docs, slices, ctx)
        assert all(d.doc_id != "auth-guide" for d in drift)

    def test_rebase_after_stamp_not_stale(self, repo: Path, ctx: cli.Ctx):
        # REGRESSION: rewriting history makes the stamped SHA vanish. The
        # fingerprint survives because file contents are unchanged.
        assert cli.main(["--repo", str(repo), "stamp", "auth-guide"]) == 0
        subprocess.run(["git", "commit", "--amend", "-m", "reworded", "--allow-empty"],
                       cwd=repo, capture_output=True, check=True)
        manifest = cli.load_doc_manifest(ctx)
        slices = cli.load_slice_docs(ctx)
        drift = cli.check_doc_drift(manifest.docs, slices, ctx)
        assert all(d.doc_id != "auth-guide" for d in drift)

    def test_legacy_sha_fallback_flags_drift(self, repo: Path, ctx: cli.Ctx):
        # A manifest entry with verified_at but no fingerprint (the fixture)
        # still gets evaluated via the SHA-diff fallback.
        (repo / "src" / "api" / "handlers.py").write_text("def get_user():\n    return 1\n")
        subprocess.run(["git", "add", "-A"], cwd=repo, capture_output=True, check=True)
        subprocess.run(["git", "commit", "-m", "edit api"], cwd=repo, capture_output=True, check=True)
        manifest = cli.load_doc_manifest(ctx)
        slices = cli.load_slice_docs(ctx)
        drift = cli.check_doc_drift(manifest.docs, slices, ctx)
        assert any(d.doc_id == "api-ref" for d in drift)
        assert all(td.fingerprint == "" for td in manifest.docs)  # none migrated yet

    def test_stamp_no_manifest(self, repo: Path):
        (repo / "slices" / "DOCS.yaml").unlink()
        rc = cli.main(["--repo", str(repo), "stamp", "auth-guide"])
        assert rc == 2

    def test_stamp_bad_doc_id(self, repo: Path):
        rc = cli.main(["--repo", str(repo), "stamp", "nope-doc"])
        assert rc == 1


# ---------------------------------------------------------------------------
# Check (integration)
# ---------------------------------------------------------------------------

class TestCheck:
    def test_clean_state_passes(self, ctx: cli.Ctx):
        docs = cli.load_slice_docs(ctx)
        result = cli.run_check(docs, ctx, staleness=False)
        assert result.ok

    def test_missing_doc_in_manifest_is_error(self, ctx: cli.Ctx):
        _write_manifest(ctx, {
            "nonexistent-doc": {"path": "nonexistent.md", "slices": ["auth-service"], "verified_at": "abc"},
        })
        docs = cli.load_slice_docs(ctx)
        result = cli.run_check(docs, ctx, staleness=False)
        assert any("doc missing" in e for e in result.errors)

    def test_unknown_slice_in_manifest_is_error(self, ctx: cli.Ctx):
        _write_manifest(ctx, {
            "auth-guide": {"path": "auth-guide.md", "slices": ["nonexistent-slice"], "verified_at": "abc"},
        })
        docs = cli.load_slice_docs(ctx)
        result = cli.run_check(docs, ctx, staleness=False)
        assert any("unknown slice" in e for e in result.errors)

    def test_doc_drift_is_warning(self, repo: Path, ctx: cli.Ctx):
        _make_drift(repo)
        docs = cli.load_slice_docs(ctx)
        result = cli.run_check(docs, ctx, staleness=False)
        assert any("doc stale" in w for w in result.warnings)

    def test_doc_drift_disabled(self, repo: Path, ctx: cli.Ctx):
        _make_drift(repo)
        docs = cli.load_slice_docs(ctx)
        result = cli.run_check(docs, ctx, staleness=False, doc_drift=False)
        assert not any("doc stale" in w for w in result.warnings)

    def test_no_manifest_is_fine(self, ctx: cli.Ctx):
        ctx.docs_manifest_path.unlink()
        docs = cli.load_slice_docs(ctx)
        result = cli.run_check(docs, ctx, staleness=False)
        assert result.ok  # No manifest = no doc checks, not an error

    def test_source_fingerprint_tracks_dirty_slice_sources(self, repo: Path, ctx: cli.Ctx):
        docs = cli.load_slice_docs(ctx)
        clean = fingerprint.source_fingerprint(docs, ctx)
        assert len(clean) == 64  # content hash, clean or dirty

        (repo / "src" / "auth" / "middleware.py").write_text("def verify_token(): return False\n")
        dirty = fingerprint.source_fingerprint(docs, ctx)
        assert dirty != clean
        assert len(dirty) == 64

    def test_index_fingerprint_stable_across_dirty_then_commit(self, repo: Path, ctx: cli.Ctx):
        # REGRESSION (review): sync while dirty, then commit the same content.
        # INDEX must NOT report stale afterward — the content hash is unchanged.
        (repo / "src" / "auth" / "middleware.py").write_text("def verify_token(): return 1\n")
        assert cli.main(["--repo", str(repo), "sync-index"]) == 0
        subprocess.run(["git", "add", "-A"], cwd=repo, capture_output=True, check=True)
        subprocess.run(["git", "commit", "-m", "edit"], cwd=repo, capture_output=True, check=True)
        docs = cli.load_slice_docs(ctx)
        assert not any("INDEX.md stale" in w for w in cli.run_check(docs, ctx).warnings)

    def test_staleness_uses_source_fingerprint_for_dirty_worktree(self, repo: Path, ctx: cli.Ctx):
        assert cli.main(["--repo", str(repo), "sync-index"]) == 0
        docs = cli.load_slice_docs(ctx)
        assert not any("INDEX.md stale" in warning for warning in cli.run_check(docs, ctx).warnings)

        (repo / "src" / "auth" / "middleware.py").write_text("def verify_token(): return False\n")
        docs = cli.load_slice_docs(ctx)
        assert any("INDEX.md stale" in warning for warning in cli.run_check(docs, ctx).warnings)

        assert cli.main(["--repo", str(repo), "sync-index"]) == 0
        docs = cli.load_slice_docs(ctx)
        assert not any("INDEX.md stale" in warning for warning in cli.run_check(docs, ctx).warnings)

    def test_doc_id_frontmatter_mismatch_is_error(self, ctx: cli.Ctx):
        # manifest key says "auth-guide" but doc frontmatter says "wrong-id"
        vault = ctx.docs_manifest_path.parent.parent / "docs"
        (vault / "auth-guide.md").write_text(
            "---\ndoc_id: wrong-id\ntitle: Auth Guide\n---\n# Auth Guide\n"
        )
        docs = cli.load_slice_docs(ctx)
        result = cli.run_check(docs, ctx, staleness=False)
        assert any("wrong-id" in e and "auth-guide" in e for e in result.errors)

    def test_doc_missing_doc_id_frontmatter_is_error(self, ctx: cli.Ctx):
        vault = ctx.docs_manifest_path.parent.parent / "docs"
        (vault / "auth-guide.md").write_text("---\ntitle: Auth Guide\n---\n# Auth Guide\n")
        docs = cli.load_slice_docs(ctx)
        result = cli.run_check(docs, ctx, staleness=False)
        assert any("no doc_id in frontmatter" in e for e in result.errors)


# ---------------------------------------------------------------------------
# CLI integration
# ---------------------------------------------------------------------------

class TestCLI:
    def test_list(self, repo: Path):
        assert cli.main(["--repo", str(repo), "list"]) == 0

    def test_list_json(self, repo: Path, capsys):
        assert cli.main(["--repo", str(repo), "list", "--json"]) == 0
        data = json.loads(capsys.readouterr().out)
        assert len(data) == 3
        assert all("doc_count" in item for item in data)

    def test_show_includes_manifest_docs(self, repo: Path, capsys):
        assert cli.main(["--repo", str(repo), "show", "auth-service", "--json"]) == 0
        data = json.loads(capsys.readouterr().out)
        assert len(data["docs"]) == 1
        assert data["docs"][0]["doc_id"] == "auth-guide"
        assert data["docs"][0]["path"] == "auth-guide.md"

    def test_stale_docs_clean(self, repo: Path, capsys):
        assert cli.main(["--repo", str(repo), "stale-docs"]) == 0
        assert "up to date" in capsys.readouterr().out

    def test_stale_docs_with_drift(self, repo: Path, capsys):
        _make_drift(repo)
        assert cli.main(["--repo", str(repo), "stale-docs", "--json"]) == 1
        data = json.loads(capsys.readouterr().out)
        assert len(data) == 1
        assert data[0]["doc_id"] == "auth-guide"
        assert data[0]["path"] == "auth-guide.md"
        assert "auth-service" in data[0]["affected_slices"]

    def test_docs_command(self, repo: Path, capsys):
        assert cli.main(["--repo", str(repo), "docs", "auth-service"]) == 0
        assert "ok" in capsys.readouterr().out

    def test_find_by_tag(self, repo: Path, capsys):
        assert cli.main(["--repo", str(repo), "find", "middleware"]) == 0
        out = capsys.readouterr().out
        assert "auth-service" in out
        assert "doc_tags" in out

    def test_for_command(self, repo: Path, capsys):
        assert cli.main(["--repo", str(repo), "for", "src/auth/middleware.py"]) == 0
        assert "auth-service" in capsys.readouterr().out

    def test_check_json(self, repo: Path, capsys):
        assert cli.main(["--repo", str(repo), "check", "--json", "--no-staleness"]) == 0
        data = json.loads(capsys.readouterr().out)
        assert data["ok"] is True

    def test_unknown_slice(self, repo: Path):
        assert cli.main(["--repo", str(repo), "show", "nope"]) == 2


# ---------------------------------------------------------------------------
# Ctx
# ---------------------------------------------------------------------------

class TestCtx:
    def test_head_sha(self, repo: Path):
        c = cli.Ctx(repo=str(repo))
        assert len(c.head_sha()) == 40

    def test_docs_manifest_path(self, repo: Path):
        c = cli.Ctx(repo=str(repo))
        assert c.docs_manifest_path == repo / "slices" / "DOCS.yaml"

    def test_rel(self, repo: Path):
        c = cli.Ctx(repo=str(repo))
        assert c.rel(repo / "src" / "auth" / "middleware.py") == "src/auth/middleware.py"


# ---------------------------------------------------------------------------
# affected-docs command
# ---------------------------------------------------------------------------

class TestAffectedDocs:
    def test_finds_linked_docs_when_stale(self, repo: Path, capsys):
        _make_drift(repo)
        rc = cli.main(["--repo", str(repo), "affected-docs", "src/auth/middleware.py", "--json"])
        assert rc == 1  # found docs
        data = json.loads(capsys.readouterr().out)
        assert len(data) == 1
        assert data[0]["doc_id"] == "auth-guide"
        assert data[0]["status"] == "stale"
        assert "auth-service" in data[0]["matching_slices"]

    def test_finds_linked_docs_when_current(self, repo: Path, capsys):
        rc = cli.main(["--repo", str(repo), "affected-docs", "src/auth/middleware.py", "--json"])
        assert rc == 0
        data = json.loads(capsys.readouterr().out)
        assert len(data) == 1
        assert data[0]["doc_id"] == "auth-guide"
        assert data[0]["status"] == "current"

    def test_unknown_file_returns_empty(self, repo: Path, capsys):
        rc = cli.main(["--repo", str(repo), "affected-docs", "src/unknown/file.py", "--json"])
        assert rc == 0  # no affected docs
        data = json.loads(capsys.readouterr().out)
        assert len(data) == 0

    def test_text_output(self, repo: Path, capsys):
        _make_drift(repo)
        rc = cli.main(["--repo", str(repo), "affected-docs", "src/auth/middleware.py"])
        assert rc == 1
        out = capsys.readouterr().out
        assert "auth-guide" in out
        assert "STALE" in out

    def test_multiple_paths(self, repo: Path, capsys):
        _make_drift(repo)
        rc = cli.main([
            "--repo", str(repo), "affected-docs",
            "src/auth/middleware.py", "src/api/handlers.py", "--json",
        ])
        assert rc == 1
        data = json.loads(capsys.readouterr().out)
        doc_ids = {d["doc_id"] for d in data}
        assert "auth-guide" in doc_ids
        assert "api-ref" in doc_ids


# ---------------------------------------------------------------------------
# Slice-context feature: section extraction, `show` flags, `slice context`
# ---------------------------------------------------------------------------

_SECTION_BODY = textwrap.dedent("""\
    Intro prose about the auth slice.

    ## System Behavior
    Verifies JWTs and manages in-memory sessions.

    ## Invariants
    Tokens expire after one hour.

    ## Runtime Flows
    request -> verify_token -> session lookup -> handler

    ## Verification
    Run: pytest tests/test_auth.py

    ## Update Triggers
    When middleware.py or sessions.py change.
""")


def _add_sections(repo: Path) -> None:
    """Rewrite the auth-service slice body with standard system sections."""
    p = repo / "slices" / "auth-service.md"
    fm = textwrap.dedent("""\
        ---
        slice_id: auth-service
        description: Auth and sessions
        loc: 30
        files:
          - src/auth/middleware.py
          - src/auth/sessions.py
        dependencies: []
        ---
        """)
    p.write_text(fm + _SECTION_BODY)


def _add_second_owner(repo: Path) -> None:
    """Add a second slice that also owns middleware.py (ambiguous ownership)."""
    (repo / "slices" / "auth-extra.md").write_text(textwrap.dedent("""\
        ---
        slice_id: auth-extra
        description: Extra auth view
        loc: 5
        files:
          - src/auth/middleware.py
        dependencies: []
        ---
        Extra slice body.
    """))


class TestSectionExtraction:
    def test_extract_parses_h2_headings(self):
        sections = cli.extract_sections(_SECTION_BODY)
        assert sections["System Behavior"].startswith("Verifies JWTs")
        assert "session lookup" in sections["Runtime Flows"]
        assert "pytest" in sections["Verification"]

    def test_extract_ignores_h3_as_delimiter(self):
        body = "## Runtime Flows\nstep one\n### sub\ndetail\n"
        sections = cli.extract_sections(body)
        assert list(sections) == ["Runtime Flows"]
        assert "### sub" in sections["Runtime Flows"]

    def test_extract_empty_without_headings(self):
        assert cli.extract_sections("just prose, no headings") == {}


class TestShowSections:
    def test_show_body_includes_full_body(self, repo: Path, capsys):
        _add_sections(repo)
        assert cli.main(["--repo", str(repo), "show", "auth-service", "--body"]) == 0
        out = capsys.readouterr().out
        assert "## System Behavior" in out
        assert "Intro prose" in out

    def test_show_system_only_standard_sections(self, repo: Path, capsys):
        _add_sections(repo)
        assert cli.main(["--repo", str(repo), "show", "auth-service", "--system"]) == 0
        out = capsys.readouterr().out
        assert "System Behavior:" in out
        assert "Update Triggers:" in out
        # metadata fields should NOT appear in section mode
        assert "doc_path:" not in out

    def test_show_call_stacks_only_runtime_flows(self, repo: Path, capsys):
        _add_sections(repo)
        assert cli.main(["--repo", str(repo), "show", "auth-service", "--call-stacks"]) == 0
        out = capsys.readouterr().out
        assert "Runtime Flows:" in out
        assert "System Behavior:" not in out

    def test_show_verification_sections(self, repo: Path, capsys):
        _add_sections(repo)
        assert cli.main(["--repo", str(repo), "show", "auth-service", "--verification"]) == 0
        out = capsys.readouterr().out
        assert "Verification:" in out
        assert "Update Triggers:" in out
        assert "Runtime Flows:" not in out

    def test_show_missing_section_does_not_fail(self, repo: Path, capsys):
        # default fixture body has no standard sections
        assert cli.main(["--repo", str(repo), "show", "auth-service", "--system"]) == 0
        assert "(not present)" in capsys.readouterr().out

    def test_show_json_sections(self, repo: Path, capsys):
        _add_sections(repo)
        assert cli.main(["--repo", str(repo), "show", "auth-service", "--system", "--json"]) == 0
        data = json.loads(capsys.readouterr().out)
        assert data["slice_id"] == "auth-service"
        assert "Runtime Flows" in data["sections"]

    def test_plain_show_unchanged(self, repo: Path, capsys):
        # backward compat: no flags -> metadata output with doc_path
        assert cli.main(["--repo", str(repo), "show", "auth-service", "--json"]) == 0
        data = json.loads(capsys.readouterr().out)
        assert data["doc_path"].endswith("auth-service.md")
        assert "sections" not in data


class TestContext:
    def test_context_by_file_resolves_slice(self, repo: Path, capsys):
        assert cli.main(["--repo", str(repo), "context", "src/auth/middleware.py"]) == 0
        out = capsys.readouterr().out
        assert "slice: auth-service" in out

    def test_context_by_slice_json_sections(self, repo: Path, capsys):
        _add_sections(repo)
        assert cli.main(["--repo", str(repo), "context", "auth-service", "--json"]) == 0
        data = json.loads(capsys.readouterr().out)
        assert len(data["slices"]) == 1
        s = data["slices"][0]
        assert s["slice_id"] == "auth-service"
        assert "Runtime Flows" in s["sections"]
        assert s["docs"][0]["doc_id"] == "auth-guide"

    def test_context_missing_sections_ok(self, repo: Path, capsys):
        assert cli.main(["--repo", str(repo), "context", "auth-service"]) == 0
        assert "(not present)" in capsys.readouterr().out

    def test_context_no_owner_fails(self, repo: Path, capsys):
        rc = cli.main(["--repo", str(repo), "context", "src/nope/missing.py"])
        assert rc == 1
        assert "no owning slice" in capsys.readouterr().err

    def test_context_strict_ambiguous_fails(self, repo: Path, capsys):
        _add_second_owner(repo)
        rc = cli.main(["--repo", str(repo), "context", "src/auth/middleware.py"])
        assert rc == 1
        err = capsys.readouterr().err
        assert "ambiguous" in err
        assert "auth-extra" in err and "auth-service" in err


class TestContextConfig:
    def test_missing_config_defaults_strict(self, repo: Path):
        _add_second_owner(repo)
        # no config.yaml -> strict -> ambiguous file errors
        assert cli.main(["--repo", str(repo), "context", "src/auth/middleware.py"]) == 1

    def test_config_best_effort_allows_multi_owner(self, repo: Path, capsys):
        _add_second_owner(repo)
        (repo / "slices" / "config.yaml").write_text("context:\n  ambiguity: best_effort\n")
        rc = cli.main(["--repo", str(repo), "context", "src/auth/middleware.py", "--json"])
        assert rc == 0
        data = json.loads(capsys.readouterr().out)
        ids = {s["slice_id"] for s in data["slices"]}
        assert ids == {"auth-extra", "auth-service"}

    def test_cli_strict_overrides_best_effort_config(self, repo: Path, capsys):
        _add_second_owner(repo)
        (repo / "slices" / "config.yaml").write_text("context:\n  ambiguity: best_effort\n")
        rc = cli.main(["--repo", str(repo), "context", "src/auth/middleware.py", "--strict"])
        assert rc == 1
        assert "ambiguous" in capsys.readouterr().err

    def test_invalid_config_value_fails(self, repo: Path, capsys):
        (repo / "slices" / "config.yaml").write_text("context:\n  ambiguity: loose\n")
        rc = cli.main(["--repo", str(repo), "context", "auth-service"])
        assert rc == 2
        err = capsys.readouterr().err
        assert "loose" in err
        assert "strict" in err and "best_effort" in err
        assert "config.yaml" in err


class TestContextHelp:
    def test_help_advertises_context(self, capsys):
        with pytest.raises(SystemExit):
            cli.main(["--help"])
        assert "context" in capsys.readouterr().out

    def test_context_help_has_examples(self, capsys):
        with pytest.raises(SystemExit):
            cli.main(["context", "--help"])
        out = capsys.readouterr().out
        assert "examples:" in out
        assert "slice context" in out
        assert "best-effort" in out

    def test_show_help_has_section_flags(self, capsys):
        with pytest.raises(SystemExit):
            cli.main(["show", "--help"])
        out = capsys.readouterr().out
        assert "--call-stacks" in out
        assert "--verification" in out


# ---------------------------------------------------------------------------
# Robustness: graceful errors, not-a-repo, env vars
# ---------------------------------------------------------------------------

class TestRobustness:
    def test_malformed_docs_yaml_exits_2(self, repo: Path, capsys):
        (repo / "slices" / "DOCS.yaml").write_text("docs: [1, 2, 3\nvault_root: ../docs\n")
        rc = cli.main(["--repo", str(repo), "stale-docs"])
        assert rc == 2
        err = capsys.readouterr().err
        assert "failed to parse" in err
        assert "DOCS.yaml" in err
        assert "Traceback" not in err

    def test_malformed_slice_frontmatter_exits_2(self, repo: Path, capsys):
        (repo / "slices" / "auth-service.md").write_text(
            "---\nslice_id: [unclosed\n---\nbody\n"
        )
        rc = cli.main(["--repo", str(repo), "list"])
        assert rc == 2
        err = capsys.readouterr().err
        assert "failed to parse" in err
        assert "auth-service.md" in err

    def test_not_a_git_repo_exits_2(self, tmp_path: Path, capsys, monkeypatch):
        # No --repo, no SLICES_REPO_ROOT, cwd is not a git repo.
        monkeypatch.delenv("SLICES_REPO_ROOT", raising=False)
        monkeypatch.delenv("SLICES_DIR", raising=False)
        monkeypatch.chdir(tmp_path)
        rc = cli.main(["list"])
        assert rc == 2
        assert "git repository" in capsys.readouterr().err

    def test_env_var_repo_root(self, repo: Path, tmp_path_factory, capsys, monkeypatch):
        # SLICES_REPO_ROOT fallback when --repo is omitted.
        elsewhere = tmp_path_factory.mktemp("elsewhere")
        monkeypatch.chdir(elsewhere)
        monkeypatch.setenv("SLICES_REPO_ROOT", str(repo))
        monkeypatch.delenv("SLICES_DIR", raising=False)
        assert cli.main(["list"]) == 0
        assert "auth-service" in capsys.readouterr().out

    def test_stale_docs_help_documents_exit_codes(self, capsys):
        with pytest.raises(SystemExit):
            cli.main(["stale-docs", "--help"])
        out = capsys.readouterr().out
        assert "exit codes:" in out
        assert "stale" in out


# ---------------------------------------------------------------------------
# slice init — repo adoption
# ---------------------------------------------------------------------------

class TestInit:
    def test_init_writes_agent_block(self, repo: Path):
        assert cli.main(["--repo", str(repo), "init"]) == 0
        text = (repo / "CLAUDE.md").read_text()
        assert "<!-- slice-cli:start -->" in text
        assert "<!-- slice-cli:end -->" in text
        assert "slice context" in text

    def test_init_idempotent(self, repo: Path):
        cli.main(["--repo", str(repo), "init"])
        cli.main(["--repo", str(repo), "init"])
        text = (repo / "CLAUDE.md").read_text()
        assert text.count("<!-- slice-cli:start -->") == 1
        assert text.count("<!-- slice-cli:end -->") == 1

    def test_init_preserves_existing_claudemd(self, repo: Path):
        (repo / "CLAUDE.md").write_text("# My Project\n\nExisting instructions.\n")
        cli.main(["--repo", str(repo), "init"])
        text = (repo / "CLAUDE.md").read_text()
        assert "Existing instructions." in text
        assert "<!-- slice-cli:start -->" in text

    def test_init_hook(self, repo: Path):
        assert cli.main(["--repo", str(repo), "init", "--hook"]) == 0
        hook = repo / ".git" / "hooks" / "pre-commit"
        assert hook.exists()
        assert "stale-docs" in hook.read_text()
        import os
        assert os.access(hook, os.X_OK)

    def test_init_ci(self, repo: Path):
        assert cli.main(["--repo", str(repo), "init", "--ci"]) == 0
        wf = repo / ".github" / "workflows" / "slice-staleness.yml"
        assert wf.exists()
        assert "slice staleness" in wf.read_text()

    def test_init_dry_run_writes_nothing(self, repo: Path, capsys):
        assert cli.main(["--repo", str(repo), "init", "--dry-run"]) == 0
        assert not (repo / "CLAUDE.md").exists()
        assert "would write" in capsys.readouterr().out

    def test_init_updates_agents_md_when_present(self, repo: Path):
        (repo / "AGENTS.md").write_text("# Agents\n")
        cli.main(["--repo", str(repo), "init"])
        assert "<!-- slice-cli:start -->" in (repo / "AGENTS.md").read_text()
        assert "<!-- slice-cli:start -->" in (repo / "CLAUDE.md").read_text()

    def test_init_help_examples(self, capsys):
        with pytest.raises(SystemExit):
            cli.main(["init", "--help"])
        out = capsys.readouterr().out
        assert "examples:" in out
        assert "--hook" in out and "--ci" in out

    def test_init_agent_installs_skill_and_agent(self, repo: Path):
        assert cli.main(["--repo", str(repo), "init", "--agent"]) == 0
        skill = repo / ".claude" / "skills" / "slice-codebase" / "SKILL.md"
        agent = repo / ".claude" / "agents" / "codebase-slicer.md"
        assert skill.exists() and agent.exists()
        assert "name: slice-codebase" in skill.read_text()
        assert "name: codebase-slicer" in agent.read_text()

    def test_init_agent_loose_install_uses_bare_agent_name(self, repo: Path):
        # Loose (non-plugin) installs aren't namespaced — the skill must call the
        # bare `codebase-slicer`, never `slice-cli:codebase-slicer`.
        cli.main(["--repo", str(repo), "init", "--agent"])
        text = (repo / ".claude" / "skills" / "slice-codebase" / "SKILL.md").read_text()
        assert "slice-cli:codebase-slicer" not in text
        assert 'subagent_type: "codebase-slicer"' in text

    def test_init_agent_dry_run_writes_nothing(self, repo: Path, capsys):
        assert cli.main(["--repo", str(repo), "init", "--agent", "--dry-run"]) == 0
        assert not (repo / ".claude").exists()
        out = capsys.readouterr().out
        assert "slice-codebase/SKILL.md" in out and "codebase-slicer.md" in out

    def test_init_without_agent_skips_skill(self, repo: Path):
        cli.main(["--repo", str(repo), "init"])
        assert not (repo / ".claude" / "skills").exists()

    def test_embedded_templates_match_committed_files(self):
        # The plugin reads the on-disk files; `slice init --agent` writes the
        # embedded constants. They must stay byte-identical or the two install
        # channels drift.
        root = Path(init_mod.__file__).resolve().parent.parent
        assert init_mod._SLICE_CODEBASE_SKILL == (root / "skills" / "slice-codebase" / "SKILL.md").read_text()
        assert init_mod._CODEBASE_SLICER_AGENT == (root / "agents" / "codebase-slicer.md").read_text()


# ---------------------------------------------------------------------------
# Command coverage: docs-bootstrap, files, deps, grep, transitive/circular
# ---------------------------------------------------------------------------

import shutil as _shutil


class TestDocsBootstrap:
    def test_generates_manifest_from_tracks(self, repo: Path, ctx: cli.Ctx):
        (repo / "slices" / "DOCS.yaml").unlink()
        vault = repo / "vault"
        vault.mkdir()
        (vault / "guide.md").write_text(
            "---\ndoc_id: guide\ntracks:\n  - src/auth/middleware.py\n---\n# Guide\n"
        )
        rc = cli.main(["--repo", str(repo), "docs-bootstrap", str(vault)])
        assert rc == 0
        manifest = cli.load_doc_manifest(ctx)
        guide = next(td for td in manifest.docs if td.doc_id == "guide")
        assert "auth-service" in guide.slices

    def test_dry_run_writes_nothing(self, repo: Path):
        (repo / "slices" / "DOCS.yaml").unlink()
        vault = repo / "vault"
        vault.mkdir()
        (vault / "guide.md").write_text("---\ndoc_id: guide\ntracks: [src/auth/middleware.py]\n---\n#\n")
        rc = cli.main(["--repo", str(repo), "docs-bootstrap", str(vault), "--dry-run"])
        assert rc == 0
        assert not (repo / "slices" / "DOCS.yaml").exists()


class TestCommandCoverage:
    def test_files(self, repo: Path, capsys):
        assert cli.main(["--repo", str(repo), "files", "auth-service", "--json"]) == 0
        data = json.loads(capsys.readouterr().out)
        assert "src/auth/middleware.py" in data

    def test_deps_direct(self, repo: Path, capsys):
        assert cli.main(["--repo", str(repo), "deps", "api-handlers", "--json"]) == 0
        data = json.loads(capsys.readouterr().out)
        assert data["dependencies"] == ["auth-service"]

    def test_deps_reverse(self, repo: Path, capsys):
        assert cli.main(["--repo", str(repo), "deps", "auth-service", "--reverse", "--json"]) == 0
        data = json.loads(capsys.readouterr().out)
        assert "api-handlers" in data["dependencies"]

    def test_deps_transitive(self, repo: Path, capsys):
        assert cli.main(["--repo", str(repo), "deps", "api-handlers", "--transitive", "--json"]) == 0
        data = json.loads(capsys.readouterr().out)
        assert "auth-service" in data["dependencies"]

    def test_deps_transitive_handles_cycle(self, repo: Path, capsys):
        # A -> B -> A must terminate, not hang or crash.
        slices = repo / "slices"
        (slices / "cyc-a.md").write_text(
            "---\nslice_id: cyc-a\ndescription: A\nfiles: []\ndependencies: [cyc-b]\n---\nA\n"
        )
        (slices / "cyc-b.md").write_text(
            "---\nslice_id: cyc-b\ndescription: B\nfiles: []\ndependencies: [cyc-a]\n---\nB\n"
        )
        assert cli.main(["--repo", str(repo), "deps", "cyc-a", "--transitive", "--json"]) == 0
        data = json.loads(capsys.readouterr().out)
        assert "cyc-b" in data["dependencies"]

    @pytest.mark.skipif(_shutil.which("rg") is None, reason="ripgrep not installed")
    def test_grep(self, repo: Path):
        # verify_token is defined in src/auth/middleware.py (owned by auth-service)
        rc = cli.main(["--repo", str(repo), "grep", "auth-service", "verify_token"])
        assert rc == 0

    def test_grep_without_rg_is_graceful(self, repo: Path, monkeypatch, capsys):
        monkeypatch.setattr(commands_mod.shutil, "which", lambda _: None)
        rc = cli.main(["--repo", str(repo), "grep", "auth-service", "verify_token"])
        assert rc == 2
        assert "rg is required" in capsys.readouterr().err


def _write_auth_slice_with_verification(
    repo: Path, verification_lines: str, abstractions: list[str] | None = None
) -> None:
    """Rewrite auth-service.md with a `## Verification` section (and optional
    frontmatter abstractions) for verification-link tests."""
    fm = [
        "---",
        "slice_id: auth-service",
        "description: Auth and sessions",
        "loc: 30",
        "files:",
        "  - src/auth/middleware.py",
        "  - src/auth/sessions.py",
    ]
    if abstractions:
        fm.append("abstractions:")
        fm.extend(f"  - {a}" for a in abstractions)
    fm += ["dependencies: []", "---", "", "Auth slice body.", "",
           "## Verification", "", verification_lines, ""]
    (repo / "slices" / "auth-service.md").write_text("\n".join(fm))


class TestVerificationLinks:
    def test_parse_extracts_links_and_upstream(self):
        body = textwrap.dedent("""\
            ## Runtime Flows

            request -> verify_token -> handler

            ## Verification

            - `verify_token` <- tests/test_auth.py::test_valid, tests/test_auth.py::test_expired
            - `create_session` <- tests/test_sessions.py
            - upstream: design/verification-links.md

            ## Update Triggers

            When the token contract changes.
        """)
        links, upstream = cli.parse_verification(body)
        assert links == [
            ("verify_token", ["tests/test_auth.py::test_valid", "tests/test_auth.py::test_expired"]),
            ("create_session", ["tests/test_sessions.py"]),
        ]
        assert upstream == ["design/verification-links.md"]

    def test_parse_ignores_freetext_and_missing_section(self):
        assert cli.parse_verification("no sections here") == ([], [])
        body = "## Verification\n\nExercise the token with a fresh and expired token.\n"
        assert cli.parse_verification(body) == ([], [])

    def test_normalize_abstraction_strips_description(self):
        assert render._normalize_abstraction("`verify_token` — checks JWT") == "verify_token"
        assert render._normalize_abstraction("create_session - makes a session") == "create_session"
        assert render._normalize_abstraction("SessionStore") == "SessionStore"

    def test_valid_refs_pass(self, repo: Path, ctx: cli.Ctx):
        (repo / "tests").mkdir()
        (repo / "tests" / "test_auth.py").write_text("def test_valid(): pass\n")
        _write_auth_slice_with_verification(
            repo,
            "- `verify_token` <- tests/test_auth.py::test_valid\n"
            "- upstream: src/auth/middleware.py",
        )
        docs = cli.load_slice_docs(ctx)
        result = cli.run_check(docs, ctx, staleness=False)
        assert not any("verification ref missing" in e for e in result.errors)
        assert not any("verification upstream missing" in e for e in result.errors)

    def test_dangling_test_ref_is_error(self, repo: Path, ctx: cli.Ctx):
        _write_auth_slice_with_verification(
            repo, "- `verify_token` <- tests/test_missing.py::test_x"
        )
        docs = cli.load_slice_docs(ctx)
        result = cli.run_check(docs, ctx, staleness=False)
        assert any("verification ref missing: tests/test_missing.py" in e for e in result.errors)

    def test_dangling_upstream_is_error(self, repo: Path, ctx: cli.Ctx):
        _write_auth_slice_with_verification(
            repo, "- upstream: design/does-not-exist.md"
        )
        docs = cli.load_slice_docs(ctx)
        result = cli.run_check(docs, ctx, staleness=False)
        assert any("verification upstream missing: design/does-not-exist.md" in e for e in result.errors)

    def test_symbol_part_not_validated(self, repo: Path, ctx: cli.Ctx):
        # Only the file is checked; a nonexistent ::symbol is fine.
        (repo / "tests").mkdir()
        (repo / "tests" / "test_auth.py").write_text("def test_valid(): pass\n")
        _write_auth_slice_with_verification(
            repo, "- `verify_token` <- tests/test_auth.py::no_such_symbol"
        )
        docs = cli.load_slice_docs(ctx)
        result = cli.run_check(docs, ctx, staleness=False)
        assert not any("verification ref missing" in e for e in result.errors)

    def test_require_verification_flags_uncovered_abstraction(self, repo: Path, ctx: cli.Ctx):
        (repo / "tests").mkdir()
        (repo / "tests" / "test_auth.py").write_text("def test_valid(): pass\n")
        _write_auth_slice_with_verification(
            repo,
            "- `verify_token` <- tests/test_auth.py::test_valid",
            abstractions=["verify_token — checks JWT", "create_session — makes a session"],
        )
        docs = cli.load_slice_docs(ctx)
        with_flag = cli.run_check(docs, ctx, staleness=False, require_verification=True)
        assert any("abstraction not verified: create_session" in w for w in with_flag.warnings)
        assert not any("abstraction not verified: verify_token" in w for w in with_flag.warnings)
        # Silent without the opt-in flag.
        without = cli.run_check(docs, ctx, staleness=False)
        assert not any("abstraction not verified" in w for w in without.warnings)
