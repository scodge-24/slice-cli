from __future__ import annotations

import argparse
import shutil
import subprocess
import sys
from collections import Counter
from dataclasses import replace
from typing import Any

from .check import _warning_category, run_check
from .config import load_config
from .context import Ctx
from .drift import (_docs_for_slice, _resolve_tracked_concrete_files,
                    check_doc_drift)
from .fingerprint import _content_fingerprint
from .index import _generate_index
from .models import DocManifest, SliceDoc, TrackedDoc
from .paths import _expand_glob, _normalize_repo_path
from .persistence import _save_doc_manifest, load_doc_manifest, load_slice_docs
from .render import (_context_payload, _emit_json, _emit_slice_sections,
                     _print_context_human)
from .selection import (_find_matches, _owners_for_path, _resolve_slice,
                        _reverse_deps, _slice_map, _transitive_deps)


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
    # Section/body mode — backward compatible: only engages when one of
    # --body/--system/--call-stacks/--verification is set.
    if args.body or args.system or args.call_stacks or args.verification:
        return _emit_slice_sections(d, args)
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
        require_verification=args.require_verification,
    )
    hidden = result.hidden_warnings
    if args.json:
        _emit_json({
            "ok": result.ok,
            "slice_count": len(docs),
            "errors": result.errors,
            "warnings": result.warnings,
            "hidden_warnings": hidden,
            "hidden_warning_count": len(hidden),
            "hidden_warning_categories": dict(Counter(_warning_category(w) for w in hidden)),
            "strict_index": args.strict_index,
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
    if hidden and not args.strict_index:
        print(f"({len(hidden)} index drift warnings hidden — use --strict-index to show)")
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

    slices = load_slice_docs(ctx)
    by_id = _slice_map(slices)
    target_ids = {t.doc_id for t in targets}

    # Record a content fingerprint of each doc's tracked files (the staleness
    # anchor) plus the HEAD short-SHA as a human-readable note. Fingerprinting
    # captures exactly the verified content, so stamping a dirty tree is correct
    # and no commit-before-stamp guard is needed.
    updated = []
    for td in manifest.docs:
        if td.doc_id in target_ids:
            fp = _content_fingerprint(
                _resolve_tracked_concrete_files(td, by_id, ctx), ctx.repo_root
            )
            updated.append(replace(td, verified_at=short_sha, fingerprint=fp))
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


def cmd_context(args: argparse.Namespace, ctx: Ctx) -> int:
    """One-command orientation: resolve a file path (or slice id) to its owning
    slice and print metadata, linked-doc staleness, and standard system sections.
    """
    slices = load_slice_docs(ctx)
    config = load_config(ctx)
    if args.strict:
        ambiguity = "strict"
    elif args.best_effort:
        ambiguity = "best_effort"
    else:
        ambiguity = config.ambiguity

    owners = _owners_for_path(slices, args.selector, ctx)
    if owners:
        if len(owners) > 1:
            owners = sorted(owners, key=lambda s: s.slice_id)
            if ambiguity == "strict":
                ids = ", ".join(o.slice_id for o in owners)
                print(
                    f"ambiguous: multiple slices own "
                    f"{_normalize_repo_path(args.selector, ctx)}: {ids}",
                    file=sys.stderr,
                )
                return 1
        targets = owners
    else:
        try:
            targets = [_resolve_slice(slices, args.selector)]
        except KeyError:
            print(f"no owning slice for: {_normalize_repo_path(args.selector, ctx)}",
                  file=sys.stderr)
            return 1

    manifest = load_doc_manifest(ctx)
    has_manifest = bool(manifest.docs)
    drifted_ids: set[str] = set()
    if has_manifest:
        # Only the docs linked to the resolved target slice(s) — avoids
        # fingerprinting every tracked file in the repo for a single lookup.
        target_ids = {d.slice_id for d in targets}
        relevant = [td for td in manifest.docs
                    if any(sid in target_ids for sid in td.slices)]
        drifted_ids = {dr.doc_id for dr in check_doc_drift(relevant, slices, ctx)}

    if args.json:
        _emit_json({
            "slices": [
                _context_payload(d, _docs_for_slice(manifest.docs, d.slice_id),
                                 drifted_ids, ctx)
                for d in targets
            ]
        })
        return 0

    for d in targets:
        _print_context_human(d, _docs_for_slice(manifest.docs, d.slice_id),
                             drifted_ids, has_manifest, ctx)
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
    return 1 if any(r["status"] == "stale" for r in results) else 0
