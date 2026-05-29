from __future__ import annotations

import fnmatch
from collections import deque
from typing import Any

from .context import Ctx
from .models import SliceDoc, TrackedDoc
from .paths import _normalize_repo_path


def _slice_map(docs: list[SliceDoc]) -> dict[str, SliceDoc]:
    return {d.slice_id: d for d in docs}


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
