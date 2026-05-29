from __future__ import annotations

import re
from typing import Any

import yaml

from .context import Ctx
from .models import DocManifest, SliceDoc, TrackedDoc


FRONTMATTER_RE = re.compile(r"^---\n(.*?)\n---\n?", re.DOTALL)


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


def _parse_yaml(text: str, source: str) -> Any:
    """yaml.safe_load with a clean, sourced error instead of a raw traceback.

    Raises ValueError (caught by main() -> exit 2) naming the offending file.
    """
    try:
        return yaml.safe_load(text)
    except yaml.YAMLError as exc:
        detail = str(exc).replace("\n", " ")
        raise ValueError(f"failed to parse {source}: {detail}") from exc


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
        frontmatter = _parse_yaml(match.group(1), ctx.rel(doc_path))
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


def load_doc_manifest(ctx: Ctx) -> DocManifest:
    """Load slices/DOCS.yaml. Returns empty DocManifest if absent."""
    manifest_path = ctx.docs_manifest_path
    if not manifest_path.exists():
        return DocManifest(vault_root_raw=None, docs=[])

    raw = _parse_yaml(manifest_path.read_text(encoding="utf-8"), ctx.rel(manifest_path))
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
            fingerprint=str(entry.get("fingerprint", "")).strip(),
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
        if td.fingerprint:
            entry["fingerprint"] = td.fingerprint
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
