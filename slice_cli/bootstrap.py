from __future__ import annotations

import argparse
import sys
from pathlib import Path
from typing import Any

from .context import Ctx
from .drift import _resolve_track_to_slice_ids
from .models import DocManifest, TrackedDoc
from .persistence import (FRONTMATTER_RE, _parse_yaml, _save_doc_manifest,
                          _string_list, load_slice_docs)


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
            parsed = _parse_yaml(match.group(1), rel_path)
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
