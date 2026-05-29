from __future__ import annotations

import argparse
import json
import re
from typing import Any

from .context import Ctx
from .models import SliceDoc, TrackedDoc


STANDARD_SECTIONS: tuple[str, ...] = (
    "System Behavior",
    "Invariants",
    "Runtime Flows",
    "Verification",
    "Update Triggers",
)


_H2_RE = re.compile(r"^##[ \t]+(.+?)[ \t]*$")


def extract_sections(body: str) -> dict[str, str]:
    """Parse level-2 (`## `) Markdown headings from a slice body.

    Returns {heading: text} with outer blank lines trimmed. Deeper headings
    (`###` and beyond) are ignored as section delimiters and stay in the text
    of the section they fall under. Returns {} when no `## ` headings exist.
    """
    sections: dict[str, str] = {}
    current: str | None = None
    buf: list[str] = []
    for line in body.splitlines():
        m = _H2_RE.match(line)
        if m:
            if current is not None:
                sections[current] = "\n".join(buf).strip("\n")
            current = m.group(1).strip()
            buf = []
        elif current is not None:
            buf.append(line)
    if current is not None:
        sections[current] = "\n".join(buf).strip("\n")
    return sections


def _section_text(sections: dict[str, str], name: str) -> str:
    """Case-insensitive lookup of a section by standard name. '' if absent."""
    target = name.lower()
    for heading, text in sections.items():
        if heading.lower() == target:
            return text
    return ""


def _present_sections(sections: dict[str, str], names: tuple[str, ...] | list[str]) -> dict[str, str]:
    """{name: text} for each requested standard section that is present."""
    return {n: t for n in names if (t := _section_text(sections, n))}


def parse_verification(body: str) -> tuple[list[tuple[str, list[str]]], list[str]]:
    """Parse the `## Verification` section into V-model traceability links.

    Returns (links, upstream) where links is [(abstraction, [ref, ...])] and
    upstream is [doc_path, ...]. Each ref is the raw `path` or `path::symbol`
    string as written. A `- ` bullet containing ` <- ` is a link; a `- upstream:`
    bullet lists requirement/design-doc paths. Free-text lines are ignored, so
    the section stays human-friendly. See design/verification-links.md.
    """
    section = _section_text(extract_sections(body), "Verification")
    links: list[tuple[str, list[str]]] = []
    upstream: list[str] = []
    for line in section.splitlines():
        stripped = line.strip()
        if not stripped.startswith("- "):
            continue
        item = stripped[2:].strip()
        if item.lower().startswith("upstream:"):
            rest = item[len("upstream:"):]
            upstream.extend(p.strip().strip("`") for p in rest.split(",") if p.strip())
        elif " <- " in item:
            left, right = item.split(" <- ", 1)
            abstraction = left.strip().strip("`")
            refs = [r.strip().strip("`") for r in right.split(",") if r.strip()]
            if abstraction and refs:
                links.append((abstraction, refs))
    return links, upstream


def _normalize_abstraction(raw: str) -> str:
    """Reduce an abstraction label to its bare symbol name for matching.

    Frontmatter abstractions read like "verify_token — checks JWT"; verification
    links reference just "verify_token". Strip backticks and any "— description"
    (or " - description") tail.
    """
    name = raw.strip().strip("`")
    for sep in ("—", " - "):
        if sep in name:
            name = name.split(sep, 1)[0]
            break
    return name.strip().strip("`")


def _emit_json(data: Any) -> None:
    print(json.dumps(data, indent=2, sort_keys=True))


def _requested_section_names(args: argparse.Namespace) -> list[str]:
    """Ordered, de-duplicated standard sections requested by show flags."""
    names: list[str] = []
    if getattr(args, "system", False):
        names.extend(STANDARD_SECTIONS)
    if getattr(args, "call_stacks", False):
        names.append("Runtime Flows")
    if getattr(args, "verification", False):
        names.extend(("Verification", "Update Triggers"))
    return list(dict.fromkeys(names))


def _emit_slice_sections(d: SliceDoc, args: argparse.Namespace) -> int:
    """Output for `slice show` section/body flags (--body/--system/etc)."""
    if args.body:
        if args.json:
            _emit_json({"slice_id": d.slice_id, "body": d.body})
        else:
            print(d.body)
        return 0

    sections = extract_sections(d.body)
    names = _requested_section_names(args)
    if args.json:
        _emit_json({"slice_id": d.slice_id, "sections": _present_sections(sections, names)})
        return 0
    for n in names:
        text = _section_text(sections, n)
        print(f"{n}:")
        print(text if text else "  (not present)")
        print()
    return 0


def _context_payload(d: SliceDoc, docs_list: list[TrackedDoc],
                      drifted_ids: set[str], ctx: Ctx) -> dict[str, Any]:
    sections = extract_sections(d.body)
    return {
        "slice_id": d.slice_id,
        "description": d.description,
        "doc_path": ctx.rel(d.doc_path),
        "files": list(d.files),
        "dependencies": list(d.dependencies),
        "docs": [
            {"doc_id": td.doc_id, "path": td.path,
             "verified_at": td.verified_at, "stale": td.doc_id in drifted_ids}
            for td in docs_list
        ],
        "sections": _present_sections(sections, STANDARD_SECTIONS),
    }


def _print_context_human(d: SliceDoc, docs_list: list[TrackedDoc],
                         drifted_ids: set[str], has_manifest: bool, ctx: Ctx) -> None:
    print(f"slice: {d.slice_id}")
    print(f"description: {d.description}")
    print(f"doc: {ctx.rel(d.doc_path)}")
    print(f"files: {', '.join(d.files) if d.files else '(none)'}")
    print(f"dependencies: {', '.join(d.dependencies) if d.dependencies else '(none)'}")
    if has_manifest:
        print("linked docs:")
        if docs_list:
            for td in docs_list:
                status = "STALE" if td.doc_id in drifted_ids else "ok   "
                print(f"  [{status}] {td.doc_id}  ({td.path})  (verified: {td.verified_at or '(never)'})")
        else:
            print("  (none)")
    sections = extract_sections(d.body)
    for n in STANDARD_SECTIONS:
        text = _section_text(sections, n)
        print(f"{n}:")
        print(text if text else "  (not present)")
    print()
