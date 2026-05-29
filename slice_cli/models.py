from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from .context import Ctx


@dataclass(frozen=True)
class SliceDoc:
    slice_id: str
    doc_path: Path
    description: str
    loc: int | None
    files: tuple[str, ...]
    abstractions: tuple[str, ...]
    dependencies: tuple[str, ...]
    exclusions: tuple[str, ...]
    frontmatter: dict[str, Any]
    body: str


@dataclass(frozen=True)
class TrackedDoc:
    """A doc entry from slices/DOCS.yaml, keyed by doc_id."""
    doc_id: str          # manifest key — stable identifier from doc frontmatter
    path: str            # relative to vault_root
    slices: tuple[str, ...]
    verified_at: str     # HEAD short-SHA at stamp time — human-readable note only
    tags: tuple[str, ...]
    include: tuple[str, ...]   # optional: narrow to specific files within slices
    exclude: tuple[str, ...]   # optional: exclude specific files/globs
    fingerprint: str = ""      # content hash of tracked files at stamp time (staleness anchor)


@dataclass
class DocManifest:
    """Contents of slices/DOCS.yaml."""
    vault_root_raw: str | None   # as written in yaml, relative to slices_dir
    docs: list[TrackedDoc]

    def vault_root(self, ctx: Ctx) -> Path | None:
        if not self.vault_root_raw:
            return None
        return (ctx.slices_dir / self.vault_root_raw).resolve()


@dataclass
class DocDrift:
    """One stale doc."""
    doc_id: str
    path: str            # relative to vault_root
    verified_at: str
    affected_slices: list[str]
    changed_files: list[str]


@dataclass
class CheckResult:
    errors: list[str] = field(default_factory=list)
    warnings: list[str] = field(default_factory=list)
    hidden_warnings: list[str] = field(default_factory=list)

    @property
    def ok(self) -> bool:
        return not self.errors
