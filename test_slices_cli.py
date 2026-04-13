"""Tests for slices_cli.py — manifest-based doc tracking."""

from __future__ import annotations

import json
import subprocess
import textwrap
from pathlib import Path

import pytest
import yaml

import slices_cli as cli


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
        auth_docs = cli._docs_for_slice(manifest.docs, "auth-service")
        assert len(auth_docs) == 1
        assert auth_docs[0].doc_id == "auth-guide"

    def test_reverse_lookup_no_match(self, ctx: cli.Ctx):
        manifest = cli.load_doc_manifest(ctx)
        assert cli._docs_for_slice(manifest.docs, "nonexistent") == []


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
        assert rc == 1  # found docs (even if not stale — file maps to a tracked doc)
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
