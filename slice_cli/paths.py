from __future__ import annotations

import glob as globmod
from pathlib import Path

from .context import Ctx


def _normalize_repo_path(raw: str, ctx: Ctx) -> str:
    candidate = Path(raw)
    if candidate.is_absolute():
        return ctx.rel(candidate.resolve())
    return str(candidate).lstrip("./")


def _is_glob(pattern: str) -> bool:
    """True if the path spec contains shell-glob metacharacters."""
    return any(c in pattern for c in ("*", "?", "["))


def _expand_glob(pattern: str, root: Path) -> list[Path]:
    if _is_glob(pattern):
        matches = sorted(Path(m).resolve() for m in globmod.glob(str(root / pattern), recursive=True))
        if matches:
            return matches
        # A real file may legitimately contain glob metacharacters in its name
        # (e.g. a Next.js route file `app/[id]/page.tsx`). If the pattern matched
        # nothing but names an existing file, treat it as that literal file.
        literal = (root / pattern).resolve()
        return [literal] if literal.is_file() else []
    resolved = (root / pattern).resolve()
    return [resolved] if resolved.exists() else []


def _resolve_raw_path(raw: str, ctx: Ctx) -> set[str]:
    """Resolve one tracked-path spec to repo-relative file paths.

    Globs expand to their matching files; a plain path normalizes as-is (kept
    even if absent, so a deletion still changes a fingerprint). Shared by INDEX
    source fingerprinting and per-doc staleness anchoring so both agree on
    exactly which files back a verification.
    """
    if _is_glob(raw):
        return {ctx.rel(p) for p in _expand_glob(raw, ctx.repo_root) if p.is_file()}
    return {_normalize_repo_path(raw, ctx)}
