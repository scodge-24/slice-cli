from __future__ import annotations

import re
from typing import Any

from .context import Ctx
from .models import SliceDoc
from .paths import _resolve_raw_path
from .selection import _slice_map


INDEX_ROW_RE = re.compile(
    r"^\|\s*`(?P<slice_id>[^`]+)`\s*\|\s*(?P<description>.*?)\s*\|\s*~?(?P<loc>[\d,?]+)\s*\|\s*$"
)


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


def _slice_source_paths(docs: list[SliceDoc], ctx: Ctx) -> set[str]:
    paths: set[str] = set()
    for doc in docs:
        paths.add(ctx.rel(doc.doc_path))
        for raw_path in doc.files:
            paths |= _resolve_raw_path(raw_path, ctx)
    return paths


def _generate_index(docs: list[SliceDoc], ctx: Ctx) -> str:
    from .fingerprint import source_fingerprint
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
        f"Source fingerprint: {source_fingerprint(docs, ctx)}",
        "",
        "| Slice ID | Description | LoC |",
        "|----------|-------------|-----|",
    ]
    for d in ordered:
        lines.append(f"| `{d.slice_id}` | {d.description} | {fmt_loc(d.loc)} |")
    lines.append("")
    return "\n".join(lines)
