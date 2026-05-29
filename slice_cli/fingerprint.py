from __future__ import annotations

import hashlib
from pathlib import Path

from .context import Ctx
from .models import SliceDoc


def _content_fingerprint(rel_paths: list[str], repo_root: Path) -> str:
    """Deterministic, order-independent SHA-256 of the given files' contents.

    Missing files hash as a stable sentinel so deletions still change the
    digest. Shared by INDEX source fingerprinting and doc-staleness anchoring.
    """
    digest = hashlib.sha256()
    digest.update(b"slice-content-v1\0")
    for rel in sorted(set(rel_paths)):
        digest.update(rel.encode())
        digest.update(b"\0")
        path = repo_root / rel
        if path.is_file():
            digest.update(path.read_bytes())
        else:
            digest.update(b"<deleted>")
        digest.update(b"\0")
    return digest.hexdigest()


def _fingerprint_equal(recorded: str, current: str) -> bool:
    """Compare two INDEX fingerprints like-for-like.

    Two git SHAs (<=40 hex) may differ only in short-vs-full length, so they
    match on the shared prefix. A 64-char content digest must match exactly —
    never prefix-match a SHA against a digest (the old bug, which could both
    false-positive and, worse, false-negative).
    """
    if recorded == current:
        return True
    if len(recorded) <= 40 and len(current) <= 40:
        n = min(len(recorded), len(current))
        return n > 0 and recorded[:n] == current[:n]
    return False


def source_fingerprint(docs: list[SliceDoc], ctx: Ctx) -> str:
    """Content hash of every slice source and tracked file in the working tree."""
    from .index import _slice_source_paths

    return _content_fingerprint(list(_slice_source_paths(docs, ctx)), ctx.repo_root)
