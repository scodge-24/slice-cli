from __future__ import annotations

import fnmatch
import glob as globmod
import re
from collections import Counter
from pathlib import Path

from .context import Ctx
from .drift import _frontmatter_doc_id, check_doc_drift
from .fingerprint import _fingerprint_equal, source_fingerprint
from .index import _parse_index
from .models import CheckResult, SliceDoc
from .paths import _expand_glob, _is_glob
from .persistence import load_doc_manifest
from .render import _normalize_abstraction, parse_verification
from .selection import _slice_map


SOURCE_EXTENSIONS = frozenset((
    ".py", ".ts", ".tsx", ".js", ".jsx", ".go", ".rs", ".rb",
    ".java", ".kt", ".cs", ".c", ".cpp", ".h", ".hpp", ".swift",
    ".vue", ".svelte", ".ex", ".exs", ".erl", ".zig", ".lua",
    ".php", ".scala", ".clj", ".hs", ".ml", ".mli",
))


def _warning_category(message: str) -> str:
    if "description drift" in message:
        return "index_description_drift"
    if "loc drift" in message:
        return "index_loc_drift"
    return "other"


def run_check(
    docs: list[SliceDoc],
    ctx: Ctx,
    *,
    strict_index: bool = False,
    staleness: bool = True,
    staged_coverage: bool = True,
    doc_drift: bool = True,
    require_verification: bool = False,
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
            if _is_glob(raw_path):
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

        # Verification links: dangling refs (and opt-in coverage gaps)
        links, upstream = parse_verification(d.body)
        for _, refs in links:
            for ref in refs:
                ref_file = ref.split("::", 1)[0]
                if not (root / ref_file).exists():
                    result.errors.append(
                        f"{ctx.rel(d.doc_path)}: verification ref missing: {ref}"
                    )
        for up in upstream:
            if not (root / up).exists():
                result.errors.append(
                    f"{ctx.rel(d.doc_path)}: verification upstream missing: {up}"
                )
        if require_verification and d.abstractions:
            linked = {_normalize_abstraction(a) for a, _ in links}
            for raw_abs in d.abstractions:
                name = _normalize_abstraction(raw_abs)
                if name and name not in linked:
                    result.warnings.append(
                        f"{ctx.rel(d.doc_path)}: abstraction not verified: {name}"
                    )

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

    for d in docs:
        row = index_rows.get(d.slice_id)
        if not row:
            continue
        if row["description"] != d.description:
            result.hidden_warnings.append(
                f"INDEX.md description drift for {d.slice_id}"
            )
        if d.loc is not None and row["loc"] != d.loc:
            result.hidden_warnings.append(f"INDEX.md loc drift for {d.slice_id}")

    if strict_index:
        result.warnings.extend(result.hidden_warnings)

    # --- INDEX.md staleness ---
    if staleness and ctx.index_path.is_file():
        content = ctx.index_path.read_text(encoding="utf-8")
        sha_match = re.search(r"Last updated:\s*([0-9a-fA-F]+)", content)
        fingerprint_match = re.search(r"Source fingerprint:\s*([0-9a-fA-F]+)", content)
        if not sha_match:
            result.warnings.append("INDEX.md has no 'Last updated: <hash>' line")
        recorded = fingerprint_match.group(1).strip() if fingerprint_match else None
        if recorded is None and sha_match:
            recorded = sha_match.group(1).strip()
        if recorded is not None:
            current = source_fingerprint(docs, ctx)
            if not _fingerprint_equal(recorded, current):
                result.warnings.append(
                    f"INDEX.md stale: recorded {recorded[:12]}, source fingerprint is {current[:12]}"
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
                    if _is_glob(raw_path):
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
