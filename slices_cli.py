#!/usr/bin/env python3
"""CLI for navigating codebase slice documents with doc-staleness tracking.

Turns `slices/*.md` into a query surface for humans and agents. Doc staleness
is tracked via `slices/DOCS.yaml` — a manifest that maps doc paths to slice IDs
with a single `verified_at` SHA per doc. Docs and slice files stay clean.
"""

from __future__ import annotations

import argparse
import fnmatch
import glob as globmod
import json
import os
import re
import shutil
import subprocess
import sys
from collections import deque
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

import yaml

FRONTMATTER_RE = re.compile(r"^---\n(.*?)\n---\n?", re.DOTALL)
INDEX_ROW_RE = re.compile(
    r"^\|\s*`(?P<slice_id>[^`]+)`\s*\|\s*(?P<description>.*?)\s*\|\s*~?(?P<loc>[\d,?]+)\s*\|\s*$"
)
SOURCE_EXTENSIONS = frozenset((
    ".py", ".ts", ".tsx", ".js", ".jsx", ".go", ".rs", ".rb",
    ".java", ".kt", ".cs", ".c", ".cpp", ".h", ".hpp", ".swift",
    ".vue", ".svelte", ".ex", ".exs", ".erl", ".zig", ".lua",
    ".php", ".scala", ".clj", ".hs", ".ml", ".mli",
))


# ---------------------------------------------------------------------------
# Context — replaces module-level globals
# ---------------------------------------------------------------------------

class Ctx:
    """Lazily-resolved repo paths. Create once, pass everywhere."""

    def __init__(self, repo: str | None = None, slices_dir: str | None = None):
        self._repo = Path(repo).resolve() if repo else None
        self._slices = Path(slices_dir).resolve() if slices_dir else None

    @property
    def repo_root(self) -> Path:
        if self._repo is None:
            env = os.environ.get("SLICES_REPO_ROOT")
            if env:
                self._repo = Path(env).resolve()
            else:
                current = Path.cwd()
                for parent in [current, *current.parents]:
                    if (parent / ".git").exists():
                        self._repo = parent
                        break
                else:
                    raise RuntimeError(
                        "Not inside a git repository. Set SLICES_REPO_ROOT or use --repo."
                    )
        return self._repo

    @property
    def slices_dir(self) -> Path:
        if self._slices is None:
            env = os.environ.get("SLICES_DIR")
            self._slices = Path(env).resolve() if env else self.repo_root / "slices"
        return self._slices

    @property
    def index_path(self) -> Path:
        return self.slices_dir / "INDEX.md"

    @property
    def docs_manifest_path(self) -> Path:
        return self.slices_dir / "DOCS.yaml"

    def git(self, *args: str, check: bool = True) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            ["git", *args],
            cwd=self.repo_root,
            capture_output=True,
            text=True,
            check=check,
        )

    def head_sha(self) -> str:
        try:
            return self.git("rev-parse", "HEAD").stdout.strip()
        except (OSError, subprocess.CalledProcessError):
            return "unknown"

    def rel(self, path: Path) -> str:
        try:
            return str(path.relative_to(self.repo_root))
        except ValueError:
            return str(path)


# ---------------------------------------------------------------------------
# Data model
# ---------------------------------------------------------------------------

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
    verified_at: str
    tags: tuple[str, ...]
    include: tuple[str, ...]   # optional: narrow to specific files within slices
    exclude: tuple[str, ...]   # optional: exclude specific files/globs


@dataclass
class DocManifest:
    """Contents of slices/DOCS.yaml."""
    vault_root_raw: str | None   # as written in yaml, relative to slices_dir
    docs: list[TrackedDoc]

    def vault_root(self, ctx: Ctx) -> Path | None:
        if not self.vault_root_raw:
            return None
        return (ctx.slices_dir / self.vault_root_raw).resolve()


def _coerce_int(raw: Any) -> int | None:
    if raw is None:
        return None
    text = str(raw).strip().replace(",", "")
    try:
        return int(text) if text else None
    except ValueError:
        return None


def _string_list(raw: Any) -> tuple[str, ...]:
    if raw is None:
        return ()
    if isinstance(raw, list):
        return tuple(str(item).strip() for item in raw if str(item).strip())
    return (str(raw).strip(),) if str(raw).strip() else ()


# ---------------------------------------------------------------------------
# Loading — slices
# ---------------------------------------------------------------------------

def load_slice_docs(ctx: Ctx) -> list[SliceDoc]:
    slices_dir = ctx.slices_dir
    if not slices_dir.exists():
        return []

    docs: list[SliceDoc] = []
    for doc_path in sorted(slices_dir.glob("*.md")):
        if doc_path.name == "INDEX.md":
            continue
        content = doc_path.read_text(encoding="utf-8")
        match = FRONTMATTER_RE.match(content)
        if not match:
            raise ValueError(f"{ctx.rel(doc_path)}: missing YAML frontmatter")
        frontmatter = yaml.safe_load(match.group(1))
        if not isinstance(frontmatter, dict):
            raise ValueError(f"{ctx.rel(doc_path)}: invalid YAML frontmatter")
        body = content[match.end():].strip()

        slice_id = str(frontmatter.get("slice_id", "")).strip()
        if not slice_id:
            raise ValueError(f"{ctx.rel(doc_path)}: missing `slice_id`")

        docs.append(SliceDoc(
            slice_id=slice_id,
            doc_path=doc_path,
            description=str(frontmatter.get("description", "")).strip(),
            loc=_coerce_int(frontmatter.get("loc")),
            files=_string_list(frontmatter.get("files")),
            abstractions=_string_list(frontmatter.get("abstractions")),
            dependencies=_string_list(frontmatter.get("dependencies")),
            exclusions=_string_list(frontmatter.get("exclusions")),
            frontmatter=frontmatter,
            body=body,
        ))
    return docs


# ---------------------------------------------------------------------------
# Loading — doc manifest
# ---------------------------------------------------------------------------

def load_doc_manifest(ctx: Ctx) -> DocManifest:
    """Load slices/DOCS.yaml. Returns empty DocManifest if absent."""
    manifest_path = ctx.docs_manifest_path
    if not manifest_path.exists():
        return DocManifest(vault_root_raw=None, docs=[])

    raw = yaml.safe_load(manifest_path.read_text(encoding="utf-8"))
    if not isinstance(raw, dict):
        return DocManifest(vault_root_raw=None, docs=[])

    vault_root_raw: str | None = raw.get("vault_root") or None

    docs_section = raw.get("docs")
    if not isinstance(docs_section, dict):
        return DocManifest(vault_root_raw=vault_root_raw, docs=[])

    tracked: list[TrackedDoc] = []
    for doc_id, entry in docs_section.items():
        if not isinstance(entry, dict):
            continue
        tracked.append(TrackedDoc(
            doc_id=str(doc_id).strip(),
            path=str(entry.get("path", "")).strip(),
            slices=_string_list(entry.get("slices")),
            verified_at=str(entry.get("verified_at", "")).strip(),
            tags=_string_list(entry.get("tags")),
            include=_string_list(entry.get("include")),
            exclude=_string_list(entry.get("exclude")),
        ))
    return DocManifest(vault_root_raw=vault_root_raw, docs=tracked)


def _save_doc_manifest(manifest: DocManifest, ctx: Ctx) -> None:
    """Write the manifest back to DOCS.yaml, keyed by doc_id."""
    docs_section: dict[str, dict[str, Any]] = {}
    for td in manifest.docs:
        entry: dict[str, Any] = {
            "path": td.path,
            "slices": list(td.slices),
            "verified_at": td.verified_at,
        }
        if td.tags:
            entry["tags"] = list(td.tags)
        if td.include:
            entry["include"] = list(td.include)
        if td.exclude:
            entry["exclude"] = list(td.exclude)
        docs_section[td.doc_id] = entry

    top: dict[str, Any] = {}
    if manifest.vault_root_raw:
        top["vault_root"] = manifest.vault_root_raw
    top["docs"] = docs_section

    content = yaml.dump(top, default_flow_style=False, sort_keys=False)
    ctx.docs_manifest_path.write_text(content, encoding="utf-8")


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _slice_map(docs: list[SliceDoc]) -> dict[str, SliceDoc]:
    return {d.slice_id: d for d in docs}


def _normalize_repo_path(raw: str, ctx: Ctx) -> str:
    candidate = Path(raw)
    if candidate.is_absolute():
        return ctx.rel(candidate.resolve())
    return str(candidate).lstrip("./")


def _resolve_slice(docs: list[SliceDoc], selector: str) -> SliceDoc:
    by_id = _slice_map(docs)
    normalized = selector.strip()
    if normalized in by_id:
        return by_id[normalized]
    stem_matches = [d for d in docs if d.doc_path.stem == normalized.removesuffix(".md")]
    if len(stem_matches) == 1:
        return stem_matches[0]
    raise KeyError(f"unknown slice: {selector}")


def _reverse_deps(docs: list[SliceDoc]) -> dict[str, list[str]]:
    reverse: dict[str, list[str]] = {d.slice_id: [] for d in docs}
    for d in docs:
        for dep in d.dependencies:
            reverse.setdefault(dep, []).append(d.slice_id)
    for vals in reverse.values():
        vals.sort()
    return reverse


def _transitive_deps(start: str, adj: dict[str, tuple[str, ...]]) -> list[str]:
    ordered, seen = [], set()
    queue: deque[str] = deque(adj.get(start, ()))
    while queue:
        current = queue.popleft()
        if current in seen:
            continue
        seen.add(current)
        ordered.append(current)
        queue.extend(dep for dep in adj.get(current, ()) if dep not in seen)
    return ordered


def _owners_for_path(docs: list[SliceDoc], raw_path: str, ctx: Ctx) -> list[SliceDoc]:
    normalized = _normalize_repo_path(raw_path, ctx)
    owners = []
    for d in docs:
        for pattern in d.files:
            if normalized == pattern or fnmatch.fnmatch(normalized, pattern):
                owners.append(d)
                break
    return owners


def _find_matches(
    docs: list[SliceDoc],
    tracked_docs: list[TrackedDoc],
    needle: str,
) -> list[dict[str, Any]]:
    text = needle.lower()
    matches = []
    # Build tag index: slice_id -> set of tags from manifest docs
    slice_tags: dict[str, set[str]] = {}
    for td in tracked_docs:
        for sid in td.slices:
            slice_tags.setdefault(sid, set()).update(td.tags)

    for d in docs:
        fields: list[str] = []
        if text in d.slice_id.lower():
            fields.append("slice_id")
        if text in d.description.lower():
            fields.append("description")
        if any(text in f.lower() for f in d.files):
            fields.append("files")
        if any(text in a.lower() for a in d.abstractions):
            fields.append("abstractions")
        if any(text in dep.lower() for dep in d.dependencies):
            fields.append("dependencies")
        # Search manifest doc tags for this slice
        tags = slice_tags.get(d.slice_id, set())
        if any(text in tag.lower() for tag in tags):
            fields.append("doc_tags")
        if text in d.body.lower():
            fields.append("body")
        if fields:
            matches.append({
                "slice_id": d.slice_id,
                "description": d.description,
                "matches": fields,
            })
    return matches


def _expand_glob(pattern: str, root: Path) -> list[Path]:
    if any(c in pattern for c in ("*", "?", "[")):
        return sorted(Path(m).resolve() for m in globmod.glob(str(root / pattern), recursive=True))
    resolved = (root / pattern).resolve()
    return [resolved] if resolved.exists() else []


# ---------------------------------------------------------------------------
# Index
# ---------------------------------------------------------------------------

def _parse_index(ctx: Ctx) -> tuple[dict[str, dict[str, Any]], list[str]]:
    """Parse INDEX.md rows. Returns (rows_by_id, ordered_ids)."""
    rows: dict[str, dict[str, Any]] = {}
    order: list[str] = []
    if not ctx.index_path.exists():
        return rows, order
    for line in ctx.index_path.read_text(encoding="utf-8").splitlines():
        match = INDEX_ROW_RE.match(line)
        if not match:
            continue
        sid = match.group("slice_id")
        loc_raw = match.group("loc").replace(",", "")
        rows[sid] = {
            "description": match.group("description").strip(),
            "loc": int(loc_raw) if loc_raw != "?" else None,
        }
        order.append(sid)
    return rows, order


def _generate_index(docs: list[SliceDoc], ctx: Ctx) -> str:
    _, current_order = _parse_index(ctx)
    by_id = _slice_map(docs)

    # Preserve existing order, append new slices alphabetically
    ordered = []
    seen: set[str] = set()
    for sid in current_order:
        if sid in by_id:
            ordered.append(by_id[sid])
            seen.add(sid)
    for d in sorted(docs, key=lambda x: x.slice_id):
        if d.slice_id not in seen:
            ordered.append(d)

    fmt_loc = lambda loc: f"~{loc:,}" if loc is not None else "~?"
    lines = [
        "# Slice Index",
        "",
        f"Last updated: {ctx.head_sha()}",
        "",
        "| Slice ID | Description | LoC |",
        "|----------|-------------|-----|",
    ]
    for d in ordered:
        lines.append(f"| `{d.slice_id}` | {d.description} | {fmt_loc(d.loc)} |")
    lines.append("")
    return "\n".join(lines)


# ---------------------------------------------------------------------------
# Doc staleness — manifest-based
# ---------------------------------------------------------------------------

@dataclass
class DocDrift:
    """One stale doc."""
    doc_id: str
    path: str            # relative to vault_root
    verified_at: str
    affected_slices: list[str]
    changed_files: list[str]


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
            # Determine which slices are actually affected
            affected = []
            for sid in linked_slices:
                s = by_id.get(sid)
                if s and any(
                    c == f or fnmatch.fnmatch(c, f)
                    for c in changed for f in s.files
                ):
                    affected.append(sid)

            drifted.append(DocDrift(
                doc_id=td.doc_id,
                path=td.path,
                verified_at=td.verified_at,
                affected_slices=affected or linked_slices,
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


# ---------------------------------------------------------------------------
# Validation (slice check)
# ---------------------------------------------------------------------------

@dataclass
class CheckResult:
    errors: list[str] = field(default_factory=list)
    warnings: list[str] = field(default_factory=list)

    @property
    def ok(self) -> bool:
        return not self.errors


def run_check(
    docs: list[SliceDoc],
    ctx: Ctx,
    *,
    strict_index: bool = False,
    staleness: bool = True,
    staged_coverage: bool = True,
    doc_drift: bool = True,
) -> CheckResult:
    root = ctx.repo_root
    result = CheckResult()
    by_id = _slice_map(docs)
    seen_ids: set[str] = set()

    # --- Per-slice structural checks ---
    for d in docs:
        if d.slice_id in seen_ids:
            result.errors.append(f"duplicate slice_id: {d.slice_id}")
        seen_ids.add(d.slice_id)

        if not d.description:
            result.errors.append(f"{ctx.rel(d.doc_path)}: missing description")
        if d.loc is None:
            result.warnings.append(f"{ctx.rel(d.doc_path)}: missing or non-numeric loc")
        if not d.files:
            result.warnings.append(f"{ctx.rel(d.doc_path)}: no files[] entries")

        # File path existence (with glob support)
        for raw_path in d.files:
            if any(c in raw_path for c in ("*", "?", "[")):
                if not globmod.glob(str(root / raw_path), recursive=True):
                    result.errors.append(f"{ctx.rel(d.doc_path)}: glob matches nothing: {raw_path}")
            else:
                if not (root / raw_path).exists():
                    result.errors.append(f"{ctx.rel(d.doc_path)}: file missing: {raw_path}")

        # ID matches filename
        if d.slice_id != d.doc_path.stem:
            result.errors.append(
                f"{ctx.rel(d.doc_path)}: slice_id '{d.slice_id}' != filename '{d.doc_path.stem}'"
            )

        # Dependencies resolve
        for dep in d.dependencies:
            if dep not in by_id and not dep.startswith("external:"):
                result.errors.append(f"{ctx.rel(d.doc_path)}: unknown dependency: {dep}")

    # --- File overlap detection ---
    file_owners: dict[str, str] = {}
    for d in docs:
        for raw_path in d.files:
            for resolved in _expand_glob(raw_path, root):
                if not resolved.is_file():
                    continue
                rel = ctx.rel(resolved)
                if rel in file_owners:
                    result.errors.append(
                        f"file overlap: {rel} in '{file_owners[rel]}' and '{d.slice_id}'"
                    )
                else:
                    file_owners[rel] = d.slice_id

    # --- INDEX.md consistency ---
    index_rows, _ = _parse_index(ctx)
    doc_ids = {d.slice_id for d in docs}
    index_ids = set(index_rows)
    missing = sorted(doc_ids - index_ids)
    extra = sorted(index_ids - doc_ids)
    if missing:
        result.errors.append(f"INDEX.md missing rows: {', '.join(missing)}")
    if extra:
        result.errors.append(f"INDEX.md stale rows: {', '.join(extra)}")

    if strict_index:
        for d in docs:
            row = index_rows.get(d.slice_id)
            if not row:
                continue
            if row["description"] != d.description:
                result.warnings.append(
                    f"INDEX.md description drift for {d.slice_id}"
                )
            if d.loc is not None and row["loc"] != d.loc:
                result.warnings.append(f"INDEX.md loc drift for {d.slice_id}")

    # --- INDEX.md staleness ---
    if staleness and ctx.index_path.is_file():
        content = ctx.index_path.read_text(encoding="utf-8")
        sha_match = re.search(r"Last updated:\s*([0-9a-fA-F]+)", content)
        if not sha_match:
            result.warnings.append("INDEX.md has no 'Last updated: <hash>' line")
        else:
            index_hash = sha_match.group(1).strip()
            head = ctx.head_sha()
            min_len = min(len(index_hash), len(head))
            if head[:min_len] != index_hash[:min_len]:
                result.warnings.append(
                    f"INDEX.md stale: recorded {index_hash[:12]}, HEAD is {head[:12]}"
                )

    # --- Staged source coverage ---
    if staged_coverage:
        try:
            proc = ctx.git("diff", "--cached", "--name-only", "--diff-filter=ACMR", check=False)
            staged = [
                line.strip() for line in proc.stdout.splitlines()
                if line.strip() and Path(line.strip()).suffix.lower() in SOURCE_EXTENSIONS
            ]
        except (OSError, FileNotFoundError):
            staged = []

        if staged:
            coverage: set[str] = set()
            glob_patterns: list[str] = []
            for d in docs:
                for raw_path in d.files:
                    if any(c in raw_path for c in ("*", "?", "[")):
                        glob_patterns.append(raw_path)
                        for resolved in _expand_glob(raw_path, root):
                            if resolved.is_file():
                                coverage.add(ctx.rel(resolved))
                    else:
                        coverage.add(raw_path)
            for rel_path in staged:
                if rel_path not in coverage:
                    if not any(fnmatch.fnmatch(rel_path, pat) for pat in glob_patterns):
                        result.warnings.append(f"staged file uncovered: {rel_path}")

    # --- Doc staleness (from manifest) ---
    if doc_drift:
        manifest = load_doc_manifest(ctx)
        if manifest.docs:
            vault_root = manifest.vault_root(ctx)
            # Validate manifest entries
            for td in manifest.docs:
                if vault_root is not None:
                    doc_path = vault_root / td.path
                    if not doc_path.exists():
                        result.errors.append(
                            f"DOCS.yaml: doc missing: {td.doc_id} ({td.path})"
                        )
                    else:
                        fm_doc_id = _frontmatter_doc_id(doc_path)
                        if fm_doc_id is None:
                            result.errors.append(
                                f"DOCS.yaml: {td.doc_id}: doc has no doc_id in frontmatter"
                            )
                        elif fm_doc_id != td.doc_id:
                            result.errors.append(
                                f"DOCS.yaml: manifest key '{td.doc_id}' != "
                                f"frontmatter doc_id '{fm_doc_id}' in {td.path}"
                            )
                for sid in td.slices:
                    if sid not in by_id:
                        result.errors.append(
                            f"DOCS.yaml: {td.doc_id} references unknown slice: {sid}"
                        )

            for drift in check_doc_drift(manifest.docs, docs, ctx):
                changed = ", ".join(drift.changed_files[:3])
                if len(drift.changed_files) > 3:
                    changed += f" (+{len(drift.changed_files) - 3} more)"
                slices_str = ", ".join(drift.affected_slices[:3])
                if len(drift.affected_slices) > 3:
                    slices_str += f" (+{len(drift.affected_slices) - 3} more)"
                result.warnings.append(
                    f"doc stale: {drift.doc_id} "
                    f"(verified_at: {drift.verified_at[:12]}, "
                    f"slices: {slices_str}, changed: {changed})"
                )

    return result


# ---------------------------------------------------------------------------
# Commands
# ---------------------------------------------------------------------------

def _emit_json(data: Any) -> None:
    print(json.dumps(data, indent=2, sort_keys=True))


def cmd_list(args: argparse.Namespace, ctx: Ctx) -> int:
    docs = load_slice_docs(ctx)
    manifest = load_doc_manifest(ctx)
    if args.json:
        _emit_json([{
            "slice_id": d.slice_id, "description": d.description,
            "loc": d.loc, "doc_count": len(_docs_for_slice(manifest.docs, d.slice_id)),
        } for d in docs])
        return 0
    width = max((len(d.slice_id) for d in docs), default=10)
    for d in docs:
        loc = f" ({d.loc} LoC)" if d.loc is not None else ""
        n_docs = len(_docs_for_slice(manifest.docs, d.slice_id))
        doc_label = f" [{n_docs} docs]" if n_docs else ""
        print(f"{d.slice_id:<{width}}  {d.description}{loc}{doc_label}")
    return 0


def cmd_show(args: argparse.Namespace, ctx: Ctx) -> int:
    d = _resolve_slice(load_slice_docs(ctx), args.selector)
    manifest = load_doc_manifest(ctx)
    tracked = _docs_for_slice(manifest.docs, d.slice_id)
    data = {
        "slice_id": d.slice_id, "description": d.description,
        "loc": d.loc, "doc_path": ctx.rel(d.doc_path),
        "files": list(d.files), "dependencies": list(d.dependencies),
        "abstractions": list(d.abstractions), "exclusions": list(d.exclusions),
        "docs": [
            {"doc_id": td.doc_id, "path": td.path, "verified_at": td.verified_at, "tags": list(td.tags)}
            for td in tracked
        ],
    }
    if args.json:
        _emit_json(data)
    else:
        for key, val in data.items():
            if isinstance(val, list) and val:
                print(f"{key}:")
                for item in val:
                    print(f"  - {item}")
            elif isinstance(val, list):
                print(f"{key}: (none)")
            else:
                print(f"{key}: {val}")
    return 0


def cmd_files(args: argparse.Namespace, ctx: Ctx) -> int:
    d = _resolve_slice(load_slice_docs(ctx), args.selector)
    if args.json:
        _emit_json(list(d.files))
    else:
        for f in d.files:
            print(f)
    return 0


def cmd_deps(args: argparse.Namespace, ctx: Ctx) -> int:
    docs = load_slice_docs(ctx)
    d = _resolve_slice(docs, args.selector)
    if args.reverse:
        deps = _reverse_deps(docs).get(d.slice_id, [])
        mode = "reverse"
    elif args.transitive:
        deps = _transitive_deps(d.slice_id, {x.slice_id: x.dependencies for x in docs})
        mode = "transitive"
    else:
        deps = list(d.dependencies)
        mode = "direct"
    if args.json:
        _emit_json({"slice_id": d.slice_id, "mode": mode, "dependencies": deps})
    else:
        for dep in deps:
            print(dep)
    return 0


def cmd_for(args: argparse.Namespace, ctx: Ctx) -> int:
    docs = load_slice_docs(ctx)
    owners = _owners_for_path(docs, args.path, ctx)
    if args.json:
        _emit_json([{"slice_id": d.slice_id, "description": d.description} for d in owners])
        return 0
    if not owners:
        print(f"no owning slice for: {_normalize_repo_path(args.path, ctx)}", file=sys.stderr)
        return 1
    for d in owners:
        print(f"{d.slice_id}\t{d.description}")
    return 0


def cmd_find(args: argparse.Namespace, ctx: Ctx) -> int:
    manifest = load_doc_manifest(ctx)
    matches = _find_matches(load_slice_docs(ctx), manifest.docs, args.needle)
    if args.json:
        _emit_json(matches)
        return 0 if matches else 1
    if not matches:
        print(f"no matches for: {args.needle}", file=sys.stderr)
        return 1
    width = max(len(m["slice_id"]) for m in matches)
    for m in matches:
        fields = ",".join(m["matches"])
        print(f"{m['slice_id']:<{width}}  [{fields}]  {m['description']}")
    return 0


def cmd_grep(args: argparse.Namespace, ctx: Ctx) -> int:
    if shutil.which("rg") is None:
        print("rg is required for `slice grep`", file=sys.stderr)
        return 2
    d = _resolve_slice(load_slice_docs(ctx), args.selector)
    if not d.files:
        print(f"{d.slice_id} has no files[]", file=sys.stderr)
        return 1
    cmd = ["rg", "-n"]
    if args.ignore_case:
        cmd.append("-i")
    if args.fixed_strings:
        cmd.append("-F")
    cmd.append(args.pattern)
    root = ctx.repo_root
    expanded = []
    for pattern in d.files:
        resolved = _expand_glob(pattern, root)
        if resolved:
            expanded.extend(str(p.relative_to(root)) for p in resolved if p.is_file())
        else:
            expanded.append(pattern)
    cmd.extend(expanded)
    return subprocess.run(cmd, cwd=root, check=False).returncode


def cmd_check(args: argparse.Namespace, ctx: Ctx) -> int:
    docs = load_slice_docs(ctx)
    result = run_check(
        docs, ctx,
        strict_index=args.strict_index,
        staleness=not args.no_staleness,
        staged_coverage=not args.no_staged_coverage,
        doc_drift=not args.no_doc_drift,
    )
    if args.json:
        _emit_json({
            "ok": result.ok,
            "slice_count": len(docs),
            "errors": result.errors,
            "warnings": result.warnings,
        })
        return 0 if result.ok else 1

    status = "OK" if result.ok else "FAILED"
    print(f"{status}: checked {len(docs)} slices")
    if result.errors:
        print("Errors:")
        for e in result.errors:
            print(f"  - {e}")
    if result.warnings:
        print("Warnings:")
        for w in result.warnings:
            print(f"  - {w}")
    elif result.ok:
        print("Warnings: none")
    return 0 if result.ok else 1


def cmd_sync_index(args: argparse.Namespace, ctx: Ctx) -> int:
    docs = load_slice_docs(ctx)
    content = _generate_index(docs, ctx)
    if args.stdout:
        sys.stdout.write(content)
        return 0
    if args.check:
        current = ctx.index_path.read_text(encoding="utf-8") if ctx.index_path.exists() else ""
        if current == content:
            print("INDEX.md is in sync")
            return 0
        print("INDEX.md is out of sync", file=sys.stderr)
        return 1
    ctx.index_path.write_text(content, encoding="utf-8")
    print(f"updated {ctx.rel(ctx.index_path)}")
    return 0


def cmd_stale_docs(args: argparse.Namespace, ctx: Ctx) -> int:
    manifest = load_doc_manifest(ctx)
    slices = load_slice_docs(ctx)
    drifted = check_doc_drift(manifest.docs, slices, ctx)
    if args.json:
        _emit_json([{
            "doc_id": dr.doc_id,
            "path": dr.path,
            "verified_at": dr.verified_at,
            "affected_slices": dr.affected_slices,
            "changed_files": dr.changed_files,
        } for dr in drifted])
        return 0 if not drifted else 1

    if not drifted:
        print("all docs are up to date")
        return 0

    for dr in drifted:
        slices_str = ", ".join(dr.affected_slices)
        print(f"{dr.doc_id}  ({dr.path})  (since {dr.verified_at[:12]})  [{slices_str}]")
        for f in dr.changed_files:
            print(f"  - {f}")
    return 1


def cmd_stamp(args: argparse.Namespace, ctx: Ctx) -> int:
    manifest = load_doc_manifest(ctx)
    if not manifest.docs:
        print("no DOCS.yaml manifest found", file=sys.stderr)
        return 2

    head = ctx.head_sha()
    if head == "unknown":
        print("cannot determine HEAD", file=sys.stderr)
        return 2

    short_sha = head[:12]

    # Filter targets
    if args.doc_id:
        targets = [td for td in manifest.docs if td.doc_id == args.doc_id]
        if not targets:
            print(f"no doc with id '{args.doc_id}' in manifest", file=sys.stderr)
            return 1
    elif args.slice:
        targets = _docs_for_slice(manifest.docs, args.slice)
        if not targets:
            print(f"no docs linked to slice '{args.slice}' in manifest", file=sys.stderr)
            return 1
    elif args.doc:
        targets = [td for td in manifest.docs if td.path == args.doc]
        if not targets:
            print(f"no doc with path '{args.doc}' in manifest", file=sys.stderr)
            return 1
    elif args.stamp_all:
        targets = list(manifest.docs)
    else:
        # Stamp all stale docs
        slices = load_slice_docs(ctx)
        drifted_ids = {dr.doc_id for dr in check_doc_drift(manifest.docs, slices, ctx)}
        targets = [td for td in manifest.docs if td.doc_id in drifted_ids]
        if not targets:
            print("all docs are up to date")
            return 0

    # Update verified_at in the manifest
    updated = []
    for td in manifest.docs:
        if any(t.doc_id == td.doc_id for t in targets):
            updated.append(TrackedDoc(
                doc_id=td.doc_id,
                path=td.path,
                slices=td.slices,
                verified_at=short_sha,
                tags=td.tags,
                include=td.include,
                exclude=td.exclude,
            ))
            print(f"stamped {td.doc_id} -> {short_sha}")
        else:
            updated.append(td)

    _save_doc_manifest(DocManifest(vault_root_raw=manifest.vault_root_raw, docs=updated), ctx)
    return 0


def cmd_docs(args: argparse.Namespace, ctx: Ctx) -> int:
    """List docs linked to a slice, with staleness info."""
    slices = load_slice_docs(ctx)
    d = _resolve_slice(slices, args.selector)
    manifest = load_doc_manifest(ctx)
    slice_docs = _docs_for_slice(manifest.docs, d.slice_id)

    drifted_ids = {
        dr.doc_id for dr in check_doc_drift(slice_docs, slices, ctx)
    }

    if args.json:
        _emit_json([{
            "doc_id": td.doc_id,
            "path": td.path,
            "verified_at": td.verified_at,
            "tags": list(td.tags),
            "stale": td.doc_id in drifted_ids,
        } for td in slice_docs])
        return 0

    if not slice_docs:
        print(f"no docs linked to slice '{d.slice_id}'")
        return 0

    for td in slice_docs:
        status = "STALE" if td.doc_id in drifted_ids else "ok   "
        tags = f"  [{', '.join(td.tags)}]" if td.tags else ""
        print(f"[{status}] {td.doc_id}  ({td.path})  (verified: {td.verified_at or '(never)'}){tags}")
    return 0


def cmd_affected_docs(args: argparse.Namespace, ctx: Ctx) -> int:
    """Given changed file paths, find docs that may need updating."""
    slices = load_slice_docs(ctx)
    manifest = load_doc_manifest(ctx)

    # Find which slices own the changed files
    affected_slice_ids: set[str] = set()
    for path in args.paths:
        for owner in _owners_for_path(slices, path, ctx):
            affected_slice_ids.add(owner.slice_id)

    if not affected_slice_ids:
        if args.json:
            _emit_json([])
        else:
            print("no owning slices found for given paths")
        return 0

    # Find docs that track any of the affected slices
    affected_docs = [
        td for td in manifest.docs
        if any(sid in affected_slice_ids for sid in td.slices)
    ]

    if not affected_docs:
        if args.json:
            _emit_json([])
        else:
            print("no tracked docs for affected slices")
        return 0

    # Check staleness for affected docs
    drifted_map = {
        dr.doc_id: dr for dr in check_doc_drift(affected_docs, slices, ctx)
    }

    results = []
    for td in affected_docs:
        matching = [sid for sid in td.slices if sid in affected_slice_ids]
        dr = drifted_map.get(td.doc_id)
        results.append({
            "doc_id": td.doc_id,
            "path": td.path,
            "matching_slices": matching,
            "status": "stale" if dr else "current",
            "changed_files": dr.changed_files if dr else [],
        })

    if args.json:
        _emit_json(results)
    else:
        for r in results:
            status = "STALE" if r["status"] == "stale" else "ok   "
            slices_str = ", ".join(r["matching_slices"])
            print(f"[{status}] {r['doc_id']}  ({r['path']})  [{slices_str}]")
            for f in r["changed_files"]:
                print(f"  - {f}")
    return 1 if results else 0


def cmd_docs_bootstrap(args: argparse.Namespace, ctx: Ctx) -> int:
    """Scan a vault directory and generate slices/DOCS.yaml from tracks: frontmatter."""
    import os as _os

    # Use the path as given (don't resolve — preserves symlink-relative paths like
    # /home/user/dev/repo which may be a symlink to /mnt/c/...)
    vault_dir = Path(args.vault_dir)
    if not vault_dir.is_absolute():
        vault_dir = Path.cwd() / vault_dir
    if not vault_dir.exists():
        print(f"vault directory not found: {vault_dir}", file=sys.stderr)
        return 2

    slices = load_slice_docs(ctx)

    # vault_root relative to slices_dir (e.g. "../wiki/rust")
    vault_root_rel = _os.path.relpath(str(vault_dir), str(ctx.slices_dir))

    entries: dict[str, dict[str, Any]] = {}          # doc_id → entry dict
    unresolved: list[tuple[str, str]] = []            # (doc_id, unresolved track)
    no_tracks: list[str] = []                         # doc_ids with no tracks field

    for md_file in sorted(vault_dir.rglob("*.md")):
        rel_path = str(md_file.relative_to(vault_dir))
        content = md_file.read_text(encoding="utf-8")
        match = FRONTMATTER_RE.match(content)
        fm: dict[str, Any] = {}
        if match:
            parsed = yaml.safe_load(match.group(1))
            if isinstance(parsed, dict):
                fm = parsed

        # Derive doc_id from frontmatter or filename stem
        doc_id = str(fm.get("doc_id", "")).strip() or md_file.stem

        tracks = _string_list(fm.get("tracks"))
        slice_ids: list[str] = []

        if not tracks:
            no_tracks.append(doc_id)
        else:
            for track in tracks:
                # Skip cross-doc references (.md files)
                if track.lower().endswith(".md"):
                    continue
                resolved = _resolve_track_to_slice_ids(track, slices, ctx)
                if resolved:
                    for sid in resolved:
                        if sid not in slice_ids:
                            slice_ids.append(sid)
                else:
                    unresolved.append((doc_id, track))

        entries[doc_id] = {
            "path": rel_path,
            "slices": sorted(slice_ids),
            "tags": list(_string_list(fm.get("tags"))),
        }

    if not entries:
        print("no .md files found in vault", file=sys.stderr)
        return 1

    if args.dry_run:
        print(f"vault_root: {vault_root_rel}")
        print(f"docs found: {len(entries)}")
        mapped = sum(1 for e in entries.values() if e["slices"])
        print(f"  with slice mappings: {mapped}")
        print(f"  without mappings:    {len(entries) - mapped}")
        print()
        for doc_id, entry in sorted(entries.items()):
            slices_str = ", ".join(entry["slices"]) if entry["slices"] else "(no slices)"
            print(f"  {doc_id}")
            print(f"    path:   {entry['path']}")
            print(f"    slices: {slices_str}")
        if unresolved:
            print(f"\nunresolved tracks ({len(unresolved)}):")
            for doc_id, track in unresolved:
                print(f"  [{doc_id}] {track}")
        return 0

    # Check for existing manifest
    if ctx.docs_manifest_path.exists() and not args.force:
        print(
            f"{ctx.rel(ctx.docs_manifest_path)} already exists — use --force to overwrite",
            file=sys.stderr,
        )
        return 1

    manifest = DocManifest(
        vault_root_raw=vault_root_rel,
        docs=[
            TrackedDoc(
                doc_id=doc_id,
                path=entry["path"],
                slices=tuple(entry["slices"]),
                verified_at="",
                tags=tuple(entry["tags"]),
                include=(),
                exclude=(),
            )
            for doc_id, entry in sorted(entries.items())
        ],
    )
    _save_doc_manifest(manifest, ctx)

    mapped = sum(1 for e in entries.values() if e["slices"])
    print(f"wrote {ctx.rel(ctx.docs_manifest_path)}")
    print(f"  docs:                {len(entries)}")
    print(f"  with slice mappings: {mapped}")
    print(f"  without mappings:    {len(entries) - mapped}  (stamp manually or add slices)")
    if unresolved:
        print(f"  unresolved tracks:   {len(unresolved)}")
        for doc_id, track in unresolved[:10]:
            print(f"    [{doc_id}] {track}")
        if len(unresolved) > 10:
            print(f"    ... and {len(unresolved) - 10} more (re-run with --dry-run to see all)")
    return 0


# ---------------------------------------------------------------------------
# Parser
# ---------------------------------------------------------------------------

def _add_json(parser: argparse.ArgumentParser) -> None:
    parser.add_argument("--json", action="store_true", help="Emit JSON.")


def _add_selector(parser: argparse.ArgumentParser) -> None:
    parser.add_argument("selector", help="Slice ID or doc stem.")


def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(
        prog="slice",
        description="Navigate codebase slice documents.",
    )
    p.add_argument("--repo", metavar="DIR", help="Override repo root.")
    p.add_argument("--slices-dir", metavar="DIR", help="Override slices directory.")
    sub = p.add_subparsers(dest="command", required=True)

    # list
    sp = sub.add_parser("list", help="List all slices.")
    _add_json(sp)
    sp.set_defaults(func=cmd_list)

    # show
    sp = sub.add_parser("show", help="Show one slice.")
    _add_selector(sp)
    _add_json(sp)
    sp.set_defaults(func=cmd_show)

    # files
    sp = sub.add_parser("files", help="List files owned by a slice.")
    _add_selector(sp)
    _add_json(sp)
    sp.set_defaults(func=cmd_files)

    # deps
    sp = sub.add_parser("deps", help="Show slice dependencies.")
    _add_selector(sp)
    mode = sp.add_mutually_exclusive_group()
    mode.add_argument("--reverse", action="store_true", help="Show reverse deps.")
    mode.add_argument("--transitive", action="store_true", help="Walk full dep chain.")
    _add_json(sp)
    sp.set_defaults(func=cmd_deps)

    # for
    sp = sub.add_parser("for", help="Find slice owners for a file path.")
    sp.add_argument("path", help="Repo-relative or absolute path.")
    _add_json(sp)
    sp.set_defaults(func=cmd_for)

    # find
    sp = sub.add_parser("find", help="Search slices by keyword.")
    sp.add_argument("needle", help="Search token.")
    _add_json(sp)
    sp.set_defaults(func=cmd_find)

    # grep
    sp = sub.add_parser("grep", help="Run rg within a slice's files.")
    _add_selector(sp)
    sp.add_argument("pattern", help="Pattern for rg.")
    sp.add_argument("-i", "--ignore-case", action="store_true")
    sp.add_argument("-F", "--fixed-strings", action="store_true")
    sp.set_defaults(func=cmd_grep)

    # check
    sp = sub.add_parser("check", help="Run integrity checks.")
    sp.add_argument("--strict-index", action="store_true", help="Show index description/loc drift.")
    sp.add_argument("--no-staleness", action="store_true", help="Skip INDEX.md staleness check.")
    sp.add_argument("--no-staged-coverage", action="store_true", help="Skip staged coverage check.")
    sp.add_argument("--no-doc-drift", action="store_true", help="Skip doc staleness check.")
    _add_json(sp)
    sp.set_defaults(func=cmd_check)

    # sync-index
    sp = sub.add_parser("sync-index", help="Regenerate INDEX.md from frontmatter.")
    mode = sp.add_mutually_exclusive_group()
    mode.add_argument("--stdout", action="store_true", help="Print instead of writing.")
    mode.add_argument("--check", action="store_true", help="Exit nonzero if out of sync.")
    sp.set_defaults(func=cmd_sync_index)

    # --- Doc-tracking commands ---

    # docs
    sp = sub.add_parser("docs", help="List docs linked to a slice.")
    _add_selector(sp)
    _add_json(sp)
    sp.set_defaults(func=cmd_docs)

    # stale-docs
    sp = sub.add_parser("stale-docs", help="List all stale docs across slices.")
    _add_json(sp)
    sp.set_defaults(func=cmd_stale_docs)

    # stamp
    sp = sub.add_parser("stamp", help="Update verified_at to HEAD in DOCS.yaml.")
    sp.add_argument("doc_id", nargs="?", default=None, help="Stamp a specific doc by doc_id.")
    sp.add_argument("--slice", metavar="SLICE_ID", help="Stamp all docs for a slice.")
    sp.add_argument("--doc", metavar="PATH", help="Stamp a specific doc by vault-relative path.")
    sp.add_argument("--all", action="store_true", default=False, dest="stamp_all",
                    help="Stamp all docs regardless of staleness.")
    sp.set_defaults(func=cmd_stamp)

    # affected-docs
    sp = sub.add_parser("affected-docs", help="Find docs affected by changed file paths.")
    sp.add_argument("paths", nargs="+", metavar="PATH", help="Changed file paths.")
    _add_json(sp)
    sp.set_defaults(func=cmd_affected_docs)

    # docs-bootstrap
    sp = sub.add_parser(
        "docs-bootstrap",
        help="Scan a vault directory and generate DOCS.yaml from tracks: frontmatter.",
    )
    sp.add_argument("vault_dir", metavar="VAULT_DIR", help="Path to vault directory to scan.")
    sp.add_argument("--dry-run", action="store_true", help="Print what would be written without writing.")
    sp.add_argument("--force", action="store_true", help="Overwrite existing DOCS.yaml.")
    sp.set_defaults(func=cmd_docs_bootstrap)

    return p


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    ctx = Ctx(repo=args.repo, slices_dir=args.slices_dir)
    try:
        return args.func(args, ctx)
    except KeyError as exc:
        print(str(exc), file=sys.stderr)
        return 2
    except ValueError as exc:
        print(str(exc), file=sys.stderr)
        return 2
    except RuntimeError as exc:
        print(str(exc), file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
