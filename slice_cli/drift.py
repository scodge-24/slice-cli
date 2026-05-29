from __future__ import annotations

import fnmatch
from pathlib import Path

import yaml

from .context import Ctx
from .fingerprint import _content_fingerprint
from .models import DocDrift, SliceDoc, TrackedDoc
from .paths import _normalize_repo_path, _resolve_raw_path
from .selection import _slice_map


def _resolve_tracked_files(
    td: TrackedDoc,
    by_id: dict[str, SliceDoc],
) -> list[str]:
    """Resolve a TrackedDoc to concrete file paths from its linked slices,
    respecting include/exclude overrides."""
    if td.include:
        # Include overrides slice-level files entirely
        base_files = list(td.include)
    else:
        base_files = []
        for sid in td.slices:
            s = by_id.get(sid)
            if s:
                base_files.extend(s.files)

    if not td.exclude:
        return base_files

    # Filter out excluded patterns
    return [
        f for f in base_files
        if not any(fnmatch.fnmatch(f, pat) for pat in td.exclude)
    ]


def _resolve_tracked_concrete_files(
    td: TrackedDoc,
    by_id: dict[str, SliceDoc],
    ctx: Ctx,
) -> list[str]:
    """Resolve a TrackedDoc to concrete repo-relative files, expanding globs.

    This is the deterministic input to a doc's content fingerprint, so `stamp`
    and `check` agree on exactly which files back a verification.
    """
    out: set[str] = set()
    for raw in _resolve_tracked_files(td, by_id):
        out |= _resolve_raw_path(raw, ctx)
    return sorted(out)


def _resolve_track_to_slice_ids(
    track: str, slices: list[SliceDoc], ctx: Ctx
) -> list[str]:
    """Resolve a legacy tracks: path to matching slice IDs.

    Handles source files (exact/fnmatch), directory prefixes, and globs.
    Returns empty list if no match (caller should record as unresolved).
    """
    normalized = _normalize_repo_path(track, ctx)
    # Strip trailing slash for prefix matching
    dir_prefix = normalized.rstrip("/") + "/"
    slice_ids: list[str] = []
    for s in slices:
        matched = False
        for sf in s.files:
            sf_norm = _normalize_repo_path(sf, ctx)
            if (
                sf_norm == normalized                     # exact
                or fnmatch.fnmatch(sf_norm, normalized)   # glob pattern
                or sf_norm.startswith(dir_prefix)         # directory prefix
            ):
                matched = True
                break
        if matched and s.slice_id not in slice_ids:
            slice_ids.append(s.slice_id)
    return sorted(slice_ids)


def _affected_slices(
    changed: set[str], linked_slices: list[str], by_id: dict[str, SliceDoc]
) -> list[str]:
    """Subset of linked slices whose files actually changed."""
    affected = []
    for sid in linked_slices:
        s = by_id.get(sid)
        if s and any(
            c == f or fnmatch.fnmatch(c, f)
            for c in changed for f in s.files
        ):
            affected.append(sid)
    return affected


def _git_changed_files(files: list[str], verified_at: str, ctx: Ctx) -> set[str]:
    """Files among `files` changed since verified_at (committed) or in the
    working tree. Best-effort — empty if git cannot resolve the range."""
    changed: set[str] = set()
    if verified_at:
        try:
            proc = ctx.git("diff", "--name-only", f"{verified_at}..HEAD",
                           "--", *files, check=False)
            if proc.returncode == 0:
                changed.update(l.strip() for l in proc.stdout.splitlines() if l.strip())
        except (OSError, FileNotFoundError):
            pass
    try:
        wt = ctx.git("diff", "--name-only", "HEAD", "--", *files, check=False)
        changed.update(l.strip() for l in wt.stdout.splitlines() if l.strip())
    except (OSError, FileNotFoundError):
        pass
    return changed


def check_doc_drift(
    tracked_docs: list[TrackedDoc],
    slices: list[SliceDoc],
    ctx: Ctx,
) -> list[DocDrift]:
    """Check each manifest doc for drift against its linked slices' files."""
    by_id = _slice_map(slices)
    drifted: list[DocDrift] = []

    for td in tracked_docs:
        files = _resolve_tracked_files(td, by_id)
        if not files:
            continue

        # Resolve which slices are actually linked
        linked_slices = [sid for sid in td.slices if sid in by_id]

        # Fingerprint anchor (preferred): compare the current content hash of the
        # tracked files against the hash recorded at stamp time. Independent of
        # git history, so it survives rebases and the edit->stamp->commit order.
        # Legacy entries without a fingerprint fall through to the SHA-diff below.
        if td.fingerprint:
            concrete = _resolve_tracked_concrete_files(td, by_id, ctx)
            if _content_fingerprint(concrete, ctx.repo_root) != td.fingerprint:
                # The fingerprint is the staleness gate; narrow the report to the
                # files git says changed (matching the legacy branch and the
                # documented JSON shape), falling back to the full tracked set
                # when git cannot attribute the change.
                changed = _git_changed_files(files, td.verified_at, ctx)
                drifted.append(DocDrift(
                    doc_id=td.doc_id,
                    path=td.path,
                    verified_at=td.verified_at or "(never)",
                    affected_slices=_affected_slices(changed, linked_slices, by_id) or linked_slices,
                    changed_files=sorted(changed) if changed else concrete,
                ))
            continue

        if not td.verified_at:
            # Never verified — always stale
            drifted.append(DocDrift(
                doc_id=td.doc_id,
                path=td.path,
                verified_at="(never)",
                affected_slices=linked_slices,
                changed_files=files,
            ))
            continue

        # Check committed changes since verified_at
        changed: set[str] = set()
        try:
            proc = ctx.git(
                "diff", "--name-only",
                f"{td.verified_at}..HEAD",
                "--", *files,
                check=False,
            )
        except (OSError, FileNotFoundError):
            continue

        if proc.returncode != 0:
            drifted.append(DocDrift(
                doc_id=td.doc_id,
                path=td.path,
                verified_at=td.verified_at,
                affected_slices=linked_slices,
                changed_files=[f"(git error: unable to resolve {td.verified_at})"],
            ))
            continue

        changed.update(line.strip() for line in proc.stdout.splitlines() if line.strip())

        # Also check staged + unstaged changes
        try:
            wt_proc = ctx.git("diff", "--name-only", "HEAD", "--", *files, check=False)
            changed.update(line.strip() for line in wt_proc.stdout.splitlines() if line.strip())
        except (OSError, FileNotFoundError):
            pass

        if changed:
            drifted.append(DocDrift(
                doc_id=td.doc_id,
                path=td.path,
                verified_at=td.verified_at,
                affected_slices=_affected_slices(changed, linked_slices, by_id) or linked_slices,
                changed_files=sorted(changed),
            ))

    return drifted


def _docs_for_slice(
    tracked_docs: list[TrackedDoc],
    slice_id: str,
) -> list[TrackedDoc]:
    """Reverse lookup: which manifest docs reference this slice?"""
    return [td for td in tracked_docs if slice_id in td.slices]


def _frontmatter_doc_id(doc_path: Path) -> str | None:
    """Read the doc_id field from a markdown file's YAML frontmatter.

    Returns None if the file has no frontmatter or no doc_id field.
    """
    try:
        text = doc_path.read_text(encoding="utf-8")
    except OSError:
        return None
    if not text.startswith("---"):
        return None
    end = text.find("\n---", 3)
    if end == -1:
        return None
    try:
        fm = yaml.safe_load(text[3:end])
    except yaml.YAMLError:
        return None
    if not isinstance(fm, dict):
        return None
    val = fm.get("doc_id")
    return str(val) if val is not None else None
