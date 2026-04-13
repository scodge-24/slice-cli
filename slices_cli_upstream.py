#!/usr/bin/env python3
"""Thin CLI for navigating codebase slice documents.

This complements a generated `slices/*.md` directory by turning slice docs into a
small query surface for humans and agents. It is intentionally lightweight:
it focuses on navigation and local integrity checks, not authoritative
validation or code generation.
"""

from __future__ import annotations

import argparse
import fnmatch
import glob
import json
import os
import re
import shutil
import subprocess
import sys
from collections import Counter
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import yaml


FRONTMATTER_RE = re.compile(r"^---\n(.*?)\n---\n?", re.DOTALL)
INDEX_ROW_RE = re.compile(
    r"^\|\s*`(?P<slice_id>[^`]+)`\s*\|\s*(?P<description>.*?)\s*\|\s*~?(?P<loc>[\d,]+)\s*\|\s*$"
)

# Module-level cache so discovery only runs once per process
_REPO_ROOT: Path | None = None
_SLICES_DIR: Path | None = None
_INDEX_PATH: Path | None = None


def _discover_repo_root() -> Path:
    """Walk up from cwd to nearest .git root, or use env/flag override."""
    env = os.environ.get("SLICES_REPO_ROOT")
    if env:
        return Path(env).resolve()
    current = Path.cwd()
    for parent in [current, *current.parents]:
        if (parent / ".git").exists():
            return parent
    raise RuntimeError("Not inside a git repository. Set SLICES_REPO_ROOT or use --repo.")


def _get_repo_root() -> Path:
    global _REPO_ROOT
    if _REPO_ROOT is None:
        _REPO_ROOT = _discover_repo_root()
    return _REPO_ROOT


def _get_slices_dir() -> Path:
    global _SLICES_DIR
    if _SLICES_DIR is None:
        env = os.environ.get("SLICES_DIR")
        if env:
            _SLICES_DIR = Path(env).resolve()
        else:
            _SLICES_DIR = _get_repo_root() / "slices"
    return _SLICES_DIR


def _get_index_path() -> Path:
    global _INDEX_PATH
    if _INDEX_PATH is None:
        _INDEX_PATH = _get_slices_dir() / "INDEX.md"
    return _INDEX_PATH


def _apply_overrides(repo: str | None, slices_dir: str | None) -> None:
    """Apply --repo and --slices-dir flag overrides before any commands run."""
    global _REPO_ROOT, _SLICES_DIR, _INDEX_PATH
    if repo:
        _REPO_ROOT = Path(repo).resolve()
    if slices_dir:
        _SLICES_DIR = Path(slices_dir).resolve()
    # Always reset index path so it recomputes from the (possibly updated) slices dir
    _INDEX_PATH = None


class RichHelpFormatter(argparse.RawDescriptionHelpFormatter):
    """Preserve multi-line onboarding text and examples."""


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


def _extract_frontmatter(doc_path: Path) -> tuple[dict[str, Any], str]:
    content = doc_path.read_text(encoding="utf-8")
    match = FRONTMATTER_RE.match(content)
    if not match:
        raise ValueError(f"{doc_path.relative_to(_get_repo_root())}: missing YAML frontmatter")
    frontmatter = yaml.safe_load(match.group(1))
    if not isinstance(frontmatter, dict):
        raise ValueError(f"{doc_path.relative_to(_get_repo_root())}: invalid YAML frontmatter")
    body = content[match.end() :].strip()
    return frontmatter, body


def _coerce_int(raw: Any) -> int | None:
    if raw is None:
        return None
    text = str(raw).strip().replace(",", "")
    if not text:
        return None
    try:
        return int(text)
    except ValueError:
        return None


def _string_list(raw: Any) -> tuple[str, ...]:
    if raw is None:
        return ()
    if isinstance(raw, list):
        return tuple(str(item).strip() for item in raw if str(item).strip())
    return (str(raw).strip(),) if str(raw).strip() else ()


def load_slice_docs() -> list[SliceDoc]:
    slices_dir = _get_slices_dir()
    docs: list[SliceDoc] = []
    if not slices_dir.exists():
        return docs

    for doc_path in sorted(slices_dir.glob("*.md")):
        if doc_path.name == "INDEX.md":
            continue
        frontmatter, body = _extract_frontmatter(doc_path)
        slice_id = str(frontmatter.get("slice_id", "")).strip()
        if not slice_id:
            raise ValueError(f"{doc_path.relative_to(_get_repo_root())}: missing `slice_id`")

        docs.append(
            SliceDoc(
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
            )
        )
    return docs


def _slice_map(docs: list[SliceDoc]) -> dict[str, SliceDoc]:
    return {doc.slice_id: doc for doc in docs}


def _normalize_repo_path(raw: str) -> str:
    candidate = Path(raw)
    if candidate.is_absolute():
        try:
            return str(candidate.resolve().relative_to(_get_repo_root()))
        except ValueError:
            return str(candidate)
    return str(candidate).lstrip("./")


def _resolve_slice(docs: list[SliceDoc], selector: str) -> SliceDoc:
    by_id = _slice_map(docs)
    normalized = selector.strip()
    if normalized in by_id:
        return by_id[normalized]

    stem_matches = [doc for doc in docs if doc.doc_path.stem == normalized.removesuffix(".md")]
    if len(stem_matches) == 1:
        return stem_matches[0]

    raise KeyError(f"unknown slice: {selector}")


def _reverse_dependencies(docs: list[SliceDoc]) -> dict[str, list[str]]:
    reverse: dict[str, list[str]] = {doc.slice_id: [] for doc in docs}
    for doc in docs:
        for dep in doc.dependencies:
            reverse.setdefault(dep, []).append(doc.slice_id)
    for values in reverse.values():
        values.sort()
    return reverse


def _transitive_deps(start: str, adjacency: dict[str, tuple[str, ...]]) -> list[str]:
    ordered: list[str] = []
    seen: set[str] = set()
    stack = list(adjacency.get(start, ()))
    while stack:
        current = stack.pop(0)
        if current in seen:
            continue
        seen.add(current)
        ordered.append(current)
        stack.extend(dep for dep in adjacency.get(current, ()) if dep not in seen)
    return ordered


def _owners_for_path(docs: list[SliceDoc], raw_path: str) -> list[SliceDoc]:
    normalized = _normalize_repo_path(raw_path)
    owners = []
    for doc in docs:
        for pattern in doc.files:
            if normalized == pattern:
                owners.append(doc)
                break
            if fnmatch.fnmatch(normalized, pattern):
                owners.append(doc)
                break
    return owners


def _find_matches(docs: list[SliceDoc], needle: str) -> list[dict[str, Any]]:
    text = needle.lower()
    matches: list[dict[str, Any]] = []
    for doc in docs:
        fields: list[str] = []
        if text in doc.slice_id.lower():
            fields.append("slice_id")
        if text in doc.description.lower():
            fields.append("description")
        if any(text in item.lower() for item in doc.files):
            fields.append("files")
        if any(text in item.lower() for item in doc.abstractions):
            fields.append("abstractions")
        if any(text in item.lower() for item in doc.dependencies):
            fields.append("dependencies")
        if text in doc.body.lower():
            fields.append("body")
        if fields:
            matches.append(
                {
                    "slice_id": doc.slice_id,
                    "description": doc.description,
                    "doc_path": str(doc.doc_path.relative_to(_get_repo_root())),
                    "matches": fields,
                }
            )
    return matches


def _index_rows() -> dict[str, dict[str, Any]]:
    rows, _ = _index_rows_and_order()
    return rows


def _index_rows_and_order() -> tuple[dict[str, dict[str, Any]], list[str]]:
    rows: dict[str, dict[str, Any]] = {}
    order: list[str] = []
    index_path = _get_index_path()
    if not index_path.exists():
        return rows, order
    for line in index_path.read_text(encoding="utf-8").splitlines():
        match = INDEX_ROW_RE.match(line)
        if not match:
            continue
        slice_id = match.group("slice_id")
        rows[slice_id] = {
            "description": match.group("description").strip(),
            "loc": int(match.group("loc").replace(",", "")),
        }
        order.append(slice_id)
    return rows, order


def _format_loc(loc: int | None) -> str:
    return f"~{loc:,}" if loc is not None else "~?"


def _git_head() -> str:
    try:
        result = subprocess.run(
            ["git", "rev-parse", "HEAD"],
            cwd=_get_repo_root(),
            check=True,
            text=True,
            capture_output=True,
        )
    except (OSError, subprocess.CalledProcessError):
        return "unknown"
    return result.stdout.strip() or "unknown"


def _index_doc_order(docs: list[SliceDoc]) -> list[SliceDoc]:
    by_id = _slice_map(docs)
    _, current_order = _index_rows_and_order()

    ordered: list[SliceDoc] = []
    seen: set[str] = set()
    for slice_id in current_order:
        doc = by_id.get(slice_id)
        if doc is None:
            continue
        ordered.append(doc)
        seen.add(slice_id)

    for doc in sorted(docs, key=lambda item: item.slice_id):
        if doc.slice_id not in seen:
            ordered.append(doc)
    return ordered


def _generate_index_content(docs: list[SliceDoc]) -> str:
    lines = [
        "# Slice Index",
        "",
        f"Last updated: {_git_head()}",
        "",
        "| Slice ID | Description | LoC |",
        "|----------|-------------|-----|",
    ]
    for doc in _index_doc_order(docs):
        lines.append(f"| `{doc.slice_id}` | {doc.description} | {_format_loc(doc.loc)} |")
    lines.append("")
    return "\n".join(lines)


def _warning_category(message: str) -> str:
    if "description drift" in message:
        return "index_description_drift"
    if "loc drift" in message:
        return "index_loc_drift"
    return "other"


SOURCE_EXTENSIONS = frozenset((
    ".py", ".ts", ".tsx", ".js", ".jsx", ".go", ".rs", ".rb",
    ".java", ".kt", ".cs", ".c", ".cpp", ".h", ".hpp", ".swift",
    ".vue", ".svelte", ".ex", ".exs", ".erl", ".zig", ".lua",
    ".php", ".scala", ".clj", ".hs", ".ml", ".mli",
))


def _expand_file_pattern(pattern: str, repo_root: Path) -> list[Path]:
    """Expand a file path or glob pattern relative to repo root."""
    if any(c in pattern for c in ("*", "?", "[")):
        return sorted(Path(m).resolve() for m in glob.glob(str(repo_root / pattern), recursive=True))
    resolved = (repo_root / pattern).resolve()
    return [resolved] if resolved.exists() else []


def _check_staleness(repo_root: Path, warnings: list[str]) -> None:
    """Check INDEX.md commit hash against HEAD."""
    index_path = _get_index_path()
    if not index_path.is_file():
        return
    content = index_path.read_text(encoding="utf-8")
    match = re.search(r"Last updated:\s*([0-9a-fA-F]+)", content)
    if not match:
        warnings.append("slices/INDEX.md has no 'Last updated: <hash>' line")
        return
    index_hash = match.group(1).strip()
    try:
        proc = subprocess.run(
            ["git", "rev-parse", "HEAD"],
            cwd=repo_root, capture_output=True, text=True, check=True,
        )
    except (subprocess.CalledProcessError, FileNotFoundError):
        warnings.append("could not determine HEAD commit")
        return
    head_hash = proc.stdout.strip()
    min_len = min(len(index_hash), len(head_hash))
    if head_hash[:min_len] != index_hash[:min_len]:
        warnings.append(
            f"slices/INDEX.md may be stale: recorded {index_hash}, HEAD is {head_hash[:12]}"
        )


def _staged_source_files(repo_root: Path) -> list[str]:
    """Return repo-relative paths of staged source files."""
    try:
        proc = subprocess.run(
            ["git", "diff", "--cached", "--name-only", "--diff-filter=ACMR"],
            cwd=repo_root, capture_output=True, text=True, check=False,
        )
    except FileNotFoundError:
        return []
    if proc.returncode != 0:
        return []
    paths: list[str] = []
    for line in proc.stdout.splitlines():
        line = line.strip()
        if line and Path(line).suffix.lower() in SOURCE_EXTENSIONS:
            paths.append(line)
    return paths


def _check_staged_coverage(docs: list[SliceDoc], repo_root: Path, warnings: list[str]) -> None:
    """Warn if staged source files aren't covered by any slice."""
    staged = _staged_source_files(repo_root)
    if not staged:
        return
    # Build concrete file coverage set
    coverage: set[str] = set()
    glob_patterns: list[str] = []
    for doc in docs:
        for raw_path in doc.files:
            if any(c in raw_path for c in ("*", "?", "[")):
                glob_patterns.append(raw_path)
                for resolved in _expand_file_pattern(raw_path, repo_root):
                    if resolved.is_file():
                        try:
                            coverage.add(str(resolved.relative_to(repo_root)))
                        except ValueError:
                            pass
            else:
                coverage.add(raw_path)
    for rel_path in staged:
        if rel_path in coverage:
            continue
        if any(fnmatch.fnmatch(rel_path, pat) for pat in glob_patterns):
            continue
        warnings.append(f"staged source file not covered by any slice: {rel_path}")


def _run_check(
    docs: list[SliceDoc],
    *,
    include_index_drift: bool,
    include_staleness: bool = True,
    include_staged_coverage: bool = True,
) -> dict[str, Any]:
    repo_root = _get_repo_root()
    errors: list[str] = []
    warnings: list[str] = []
    hidden_warnings: list[str] = []

    seen_ids: set[str] = set()
    by_id = _slice_map(docs)
    for doc in docs:
        if doc.slice_id in seen_ids:
            errors.append(f"duplicate slice_id: {doc.slice_id}")
        seen_ids.add(doc.slice_id)

        if not doc.description:
            errors.append(f"{doc.doc_path.relative_to(repo_root)}: missing description")
        if doc.loc is None:
            warnings.append(f"{doc.doc_path.relative_to(repo_root)}: missing or non-numeric loc")
        if not doc.files:
            warnings.append(f"{doc.doc_path.relative_to(repo_root)}: no files[] entries")

        for raw_path in doc.files:
            if any(c in raw_path for c in ("*", "?", "[")):
                matches = glob.glob(str(repo_root / raw_path), recursive=True)
                if not matches:
                    errors.append(
                        f"{doc.doc_path.relative_to(repo_root)}: glob pattern matches no files: {raw_path}"
                    )
            else:
                resolved = repo_root / raw_path
                if not resolved.exists():
                    errors.append(
                        f"{doc.doc_path.relative_to(repo_root)}: file path does not exist: {raw_path}"
                    )

        for dep in doc.dependencies:
            if dep not in seen_ids and dep not in by_id:
                errors.append(
                    f"{doc.doc_path.relative_to(repo_root)}: unknown dependency slice: {dep}"
                )

    # File overlap detection
    file_owners: dict[str, str] = {}
    for doc in docs:
        for raw_path in doc.files:
            for resolved in _expand_file_pattern(raw_path, repo_root):
                if not resolved.is_file():
                    continue
                try:
                    rel = str(resolved.relative_to(repo_root))
                except ValueError:
                    continue
                if rel in file_owners:
                    errors.append(
                        f"file overlap: {rel} appears in both "
                        f"'{file_owners[rel]}' and '{doc.slice_id}'"
                    )
                else:
                    file_owners[rel] = doc.slice_id

    # INDEX.md row checks
    index_rows = _index_rows()
    doc_ids = {doc.slice_id for doc in docs}
    index_ids = set(index_rows)

    missing_from_index = sorted(doc_ids - index_ids)
    extra_in_index = sorted(index_ids - doc_ids)
    if missing_from_index:
        errors.append(f"slices/INDEX.md missing slice rows: {', '.join(missing_from_index)}")
    if extra_in_index:
        errors.append(f"slices/INDEX.md has stale slice rows: {', '.join(extra_in_index)}")

    for doc in docs:
        row = index_rows.get(doc.slice_id)
        if not row:
            continue
        if row["description"] != doc.description:
            hidden_warnings.append(
                f"slices/INDEX.md description drift for {doc.slice_id}: "
                f"index={row['description']!r} doc={doc.description!r}"
            )
        if doc.loc is not None and row["loc"] != doc.loc:
            hidden_warnings.append(
                f"slices/INDEX.md loc drift for {doc.slice_id}: "
                f"index={row['loc']} doc={doc.loc}"
            )

    if include_index_drift:
        warnings.extend(hidden_warnings)

    # Staleness check
    if include_staleness:
        _check_staleness(repo_root, warnings)

    # Staged coverage check
    if include_staged_coverage:
        _check_staged_coverage(docs, repo_root, warnings)

    return {
        "ok": not errors,
        "slice_count": len(docs),
        "errors": errors,
        "warnings": warnings,
        "hidden_warnings": [] if include_index_drift else hidden_warnings,
        "hidden_warning_count": 0 if include_index_drift else len(hidden_warnings),
        "hidden_warning_categories": (
            {}
            if include_index_drift
            else dict(Counter(_warning_category(item) for item in hidden_warnings))
        ),
        "strict_index": include_index_drift,
    }


def _emit(data: Any, *, as_json: bool) -> None:
    if as_json:
        print(json.dumps(data, indent=2, sort_keys=True))
        return

    if isinstance(data, list):
        for item in data:
            print(item)
        return

    if isinstance(data, dict):
        for key, value in data.items():
            if isinstance(value, list):
                print(f"{key}:")
                for item in value:
                    print(f"  - {item}")
            else:
                print(f"{key}: {value}")
        return

    print(data)


def cmd_list(args: argparse.Namespace) -> int:
    docs = load_slice_docs()
    rows = [
        {
            "slice_id": doc.slice_id,
            "description": doc.description,
            "loc": doc.loc,
            "doc_path": str(doc.doc_path.relative_to(_get_repo_root())),
        }
        for doc in docs
    ]
    if args.json:
        _emit(rows, as_json=True)
        return 0

    width = max(len(doc["slice_id"]) for doc in rows) if rows else 10
    for row in rows:
        loc = f" ({row['loc']} LoC)" if row["loc"] is not None else ""
        print(f"{row['slice_id']:<{width}}  {row['description']}{loc}")
    return 0


def cmd_show(args: argparse.Namespace) -> int:
    doc = _resolve_slice(load_slice_docs(), args.selector)
    data = {
        "slice_id": doc.slice_id,
        "description": doc.description,
        "loc": doc.loc,
        "doc_path": str(doc.doc_path.relative_to(_get_repo_root())),
        "files": list(doc.files),
        "dependencies": list(doc.dependencies),
        "abstractions": list(doc.abstractions),
        "exclusions": list(doc.exclusions),
    }
    _emit(data, as_json=args.json)
    return 0


def cmd_files(args: argparse.Namespace) -> int:
    doc = _resolve_slice(load_slice_docs(), args.selector)
    _emit(list(doc.files), as_json=args.json)
    return 0


def cmd_deps(args: argparse.Namespace) -> int:
    docs = load_slice_docs()
    doc = _resolve_slice(docs, args.selector)
    reverse = _reverse_dependencies(docs)
    if args.reverse:
        deps = reverse.get(doc.slice_id, [])
    elif args.transitive:
        deps = _transitive_deps(doc.slice_id, {d.slice_id: d.dependencies for d in docs})
    else:
        deps = list(doc.dependencies)

    if args.json:
        _emit(
            {
                "slice_id": doc.slice_id,
                "mode": "reverse" if args.reverse else "transitive" if args.transitive else "direct",
                "dependencies": deps,
            },
            as_json=True,
        )
        return 0

    for dep in deps:
        print(dep)
    return 0


def cmd_for(args: argparse.Namespace) -> int:
    docs = load_slice_docs()
    owners = _owners_for_path(docs, args.path)
    if args.json:
        _emit(
            [
                {
                    "slice_id": doc.slice_id,
                    "description": doc.description,
                    "doc_path": str(doc.doc_path.relative_to(_get_repo_root())),
                }
                for doc in owners
            ],
            as_json=True,
        )
        return 0

    if not owners:
        print(f"no owning slice found for: {_normalize_repo_path(args.path)}", file=sys.stderr)
        return 1

    for doc in owners:
        print(f"{doc.slice_id}\t{doc.description}")
    return 0


def cmd_find(args: argparse.Namespace) -> int:
    matches = _find_matches(load_slice_docs(), args.needle)
    if args.json:
        _emit(matches, as_json=True)
        return 0 if matches else 1

    if not matches:
        print(f"no slice matches for: {args.needle}", file=sys.stderr)
        return 1

    width = max(len(item["slice_id"]) for item in matches)
    for item in matches:
        fields = ",".join(item["matches"])
        print(f"{item['slice_id']:<{width}}  [{fields}]  {item['description']}")
    return 0


def cmd_grep(args: argparse.Namespace) -> int:
    if shutil.which("rg") is None:
        print("rg is required for `slice grep`", file=sys.stderr)
        return 2

    doc = _resolve_slice(load_slice_docs(), args.selector)
    if not doc.files:
        print(f"{doc.slice_id} has no files[] entries", file=sys.stderr)
        return 1

    cmd = ["rg", "-n"]
    if args.ignore_case:
        cmd.append("-i")
    if args.fixed_strings:
        cmd.append("-F")
    cmd.append(args.pattern)
    cmd.extend(doc.files)
    return subprocess.run(cmd, cwd=_get_repo_root(), check=False).returncode


def cmd_check(args: argparse.Namespace) -> int:
    docs = load_slice_docs()
    report = _run_check(
        docs,
        include_index_drift=args.strict_index,
        include_staleness=not args.no_staleness,
        include_staged_coverage=not args.no_staged_coverage,
    )
    if args.json:
        _emit(report, as_json=True)
        return 0 if report["ok"] else 1

    status = "OK" if report["ok"] else "FAILED"
    print(f"{status}: checked {report['slice_count']} slices")

    if report["errors"]:
        print("Errors:")
        for item in report["errors"]:
            print(f"  - {item}")

    if report["warnings"]:
        print("Warnings:")
        for item in report["warnings"]:
            print(f"  - {item}")
    elif not report["errors"]:
        print("Warnings: none")

    hidden_count = report["hidden_warning_count"]
    if hidden_count:
        categories = report["hidden_warning_categories"]
        summary = ", ".join(
            f"{count} {name.replace('_', ' ')}" for name, count in sorted(categories.items())
        )
        print(
            "Suppressed index drift warnings: "
            f"{hidden_count} ({summary}). "
            "Run `slice check --strict-index` to show them."
        )

    return 0 if report["ok"] else 1


def cmd_sync_index(args: argparse.Namespace) -> int:
    docs = load_slice_docs()
    content = _generate_index_content(docs)
    index_path = _get_index_path()
    current = index_path.read_text(encoding="utf-8") if index_path.exists() else ""

    if args.stdout:
        sys.stdout.write(content)
        return 0

    if args.check:
        if current == content:
            print("slices/INDEX.md is in sync")
            return 0
        print("slices/INDEX.md is out of sync", file=sys.stderr)
        return 1

    index_path.write_text(content, encoding="utf-8")
    print(f"updated {index_path.relative_to(_get_repo_root())}")
    return 0


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description=(
            "Navigate codebase slice documents.\n\n"
            "Use this CLI as a routing layer over `slices/*.md`.\n"
            "Slices help you find the right code quickly, but they are not\n"
            "runtime proof. Localize issues with runtime tools first, then use\n"
            "slice commands to bound the code-reading set.\n\n"
            "Debugging workflow:\n"
            "  1. Identify the relevant file or symbol with a runtime tool\n"
            "  2. slice find <symbol-or-term>\n"
            "  3. slice show <slice-id>\n"
            "  4. slice deps <slice-id>\n"
            "  5. slice grep <slice-id> <pattern>\n\n"
            "General development workflow:\n"
            "  1. slice for <repo-path>\n"
            "  2. slice show <slice-id>\n"
            "  3. slice deps <slice-id> --reverse\n"
            "  4. keep edits inside the owning slice unless wider changes are justified"
        ),
        epilog=(
            "Examples:\n"
            "  slice list\n"
            "  slice show auth-service\n"
            "  slice for src/auth/middleware.py\n"
            "  slice find handle_login\n"
            "  slice grep api-handlers validate_token\n"
            "  slice check\n"
            "  slice sync-index --check\n\n"
            "For command-specific onboarding, run `slice <command> --help`."
        ),
        formatter_class=RichHelpFormatter,
    )
    parser.add_argument(
        "--repo",
        metavar="DIR",
        default=None,
        help=(
            "Override the repository root (default: walk up from cwd to nearest .git, "
            "or SLICES_REPO_ROOT env var)."
        ),
    )
    parser.add_argument(
        "--slices-dir",
        metavar="DIR",
        default=None,
        help=(
            "Override the slices directory (default: <repo-root>/slices/, "
            "or SLICES_DIR env var)."
        ),
    )
    subparsers = parser.add_subparsers(dest="command", required=True)

    list_parser = subparsers.add_parser(
        "list",
        help="List all slices.",
        description=(
            "List every slice with its description and LoC.\n\n"
            "Use this when you want a quick map of the current codebase subsystems."
        ),
        epilog="Examples:\n  slice list\n  slice list --json",
        formatter_class=RichHelpFormatter,
    )
    list_parser.add_argument("--json", action="store_true", help="Emit JSON.")
    list_parser.set_defaults(func=cmd_list)

    show_parser = subparsers.add_parser(
        "show",
        help="Show one slice.",
        description=(
            "Show detailed metadata for one slice.\n\n"
            "Output includes description, LoC, files, dependencies, abstractions,\n"
            "and exclusions."
        ),
        epilog=(
            "Examples:\n"
            "  slice show auth-service\n"
            "  slice show api-handlers\n"
            "  slice show auth-service --json"
        ),
        formatter_class=RichHelpFormatter,
    )
    show_parser.add_argument("selector", help="Slice ID or doc stem.")
    show_parser.add_argument("--json", action="store_true", help="Emit JSON.")
    show_parser.set_defaults(func=cmd_show)

    files_parser = subparsers.add_parser(
        "files",
        help="List files owned by a slice.",
        description=(
            "Print only the files[] set for a slice.\n\n"
            "Use this when you want to open the owned files directly or scope\n"
            "another tool to that exact set."
        ),
        epilog=(
            "Examples:\n"
            "  slice files auth-service\n"
            "  slice files api-handlers --json"
        ),
        formatter_class=RichHelpFormatter,
    )
    files_parser.add_argument("selector", help="Slice ID or doc stem.")
    files_parser.add_argument("--json", action="store_true", help="Emit JSON.")
    files_parser.set_defaults(func=cmd_files)

    deps_parser = subparsers.add_parser(
        "deps",
        help="Show slice dependencies.",
        description=(
            "Show dependency relationships for a slice.\n\n"
            "Default mode prints direct dependencies.\n"
            "`--reverse` shows slices that depend on this slice.\n"
            "`--transitive` walks the full dependency chain."
        ),
        epilog=(
            "Examples:\n"
            "  slice deps auth-service\n"
            "  slice deps auth-service --reverse\n"
            "  slice deps auth-service --transitive\n"
            "  slice deps auth-service --json"
        ),
        formatter_class=RichHelpFormatter,
    )
    deps_parser.add_argument("selector", help="Slice ID or doc stem.")
    mode = deps_parser.add_mutually_exclusive_group()
    mode.add_argument("--reverse", action="store_true", help="Show reverse dependencies.")
    mode.add_argument("--transitive", action="store_true", help="Show transitive dependencies.")
    deps_parser.add_argument("--json", action="store_true", help="Emit JSON.")
    deps_parser.set_defaults(func=cmd_deps)

    for_parser = subparsers.add_parser(
        "for",
        help="Find slice owners for a repo path.",
        description=(
            "Resolve a file path to its owning slice.\n\n"
            "Accepts repo-relative or absolute paths. Slice file patterns may use\n"
            "fnmatch glob syntax (e.g. src/**/*.py) and will be matched accordingly."
        ),
        epilog=(
            "Examples:\n"
            "  slice for src/auth/middleware.py\n"
            "  slice for src/api/handlers/users.py\n"
            "  slice for src/auth/middleware.py --json"
        ),
        formatter_class=RichHelpFormatter,
    )
    for_parser.add_argument("path", help="Repo-relative or absolute path.")
    for_parser.add_argument("--json", action="store_true", help="Emit JSON.")
    for_parser.set_defaults(func=cmd_for)

    find_parser = subparsers.add_parser(
        "find",
        help="Find slices by token or keyword.",
        description=(
            "Search slice metadata and body text.\n\n"
            "Best when you only know a function name, module name, or subsystem term."
        ),
        epilog=(
            "Examples:\n"
            "  slice find validate_token\n"
            "  slice find middleware\n"
            "  slice find auth\n"
            "  slice find UserSession --json"
        ),
        formatter_class=RichHelpFormatter,
    )
    find_parser.add_argument("needle", help="Search token.")
    find_parser.add_argument("--json", action="store_true", help="Emit JSON.")
    find_parser.set_defaults(func=cmd_find)

    grep_parser = subparsers.add_parser(
        "grep",
        help="Run rg within a slice's file set.",
        description=(
            "Run ripgrep only against the files listed in a slice.\n\n"
            "Use this after you know the owning slice and want to avoid\n"
            "repo-wide search noise."
        ),
        epilog=(
            "Examples:\n"
            "  slice grep auth-service validate_token\n"
            "  slice grep auth-service SESSION_SECRET -F\n"
            "  slice grep api-handlers UserRequest"
        ),
        formatter_class=RichHelpFormatter,
    )
    grep_parser.add_argument("selector", help="Slice ID or doc stem.")
    grep_parser.add_argument("pattern", help="Pattern passed to rg.")
    grep_parser.add_argument("-i", "--ignore-case", action="store_true", help="Ignore case.")
    grep_parser.add_argument(
        "-F", "--fixed-strings", action="store_true", help="Treat pattern as a fixed string."
    )
    grep_parser.set_defaults(func=cmd_grep)

    check_parser = subparsers.add_parser(
        "check",
        help="Run lightweight integrity checks for slice docs.",
        description=(
            "Run lightweight integrity checks for the slices.\n\n"
            "Default mode checks:\n"
            "- duplicate slice IDs\n"
            "- missing descriptions\n"
            "- missing files[] paths (with glob support)\n"
            "- file overlaps between slices\n"
            "- unknown dependency slices\n"
            "- missing or stale rows in slices/INDEX.md\n"
            "- INDEX.md staleness (commit hash vs HEAD)\n"
            "- staged source file coverage\n\n"
            "Use `--strict-index` to also show description and LoC drift between\n"
            "slices/INDEX.md and the per-slice frontmatter.\n"
            "Use `--no-staleness` to skip INDEX.md commit hash comparison.\n"
            "Use `--no-staged-coverage` to skip staged file coverage checks."
        ),
        epilog=(
            "Examples:\n"
            "  slice check\n"
            "  slice check --strict-index\n"
            "  slice check --no-staleness --no-staged-coverage\n"
            "  slice check --json"
        ),
        formatter_class=RichHelpFormatter,
    )
    check_parser.add_argument(
        "--strict-index",
        action="store_true",
        help="Include slices/INDEX.md description/loc drift warnings.",
    )
    check_parser.add_argument(
        "--no-staleness",
        action="store_true",
        help="Skip INDEX.md staleness check (commit hash comparison).",
    )
    check_parser.add_argument(
        "--no-staged-coverage",
        action="store_true",
        help="Skip staged source file coverage checks.",
    )
    check_parser.add_argument("--json", action="store_true", help="Emit JSON.")
    check_parser.set_defaults(func=cmd_check)

    sync_parser = subparsers.add_parser(
        "sync-index",
        help="Rewrite slices/INDEX.md from per-slice frontmatter.",
        description=(
            "Regenerate slices/INDEX.md from the per-slice frontmatter.\n\n"
            "Use this after slice refreshes so the checked-in index stays aligned."
        ),
        epilog=(
            "Examples:\n"
            "  slice sync-index\n"
            "  slice sync-index --check\n"
            "  slice sync-index --stdout"
        ),
        formatter_class=RichHelpFormatter,
    )
    sync_mode = sync_parser.add_mutually_exclusive_group()
    sync_mode.add_argument("--stdout", action="store_true", help="Print the generated index.")
    sync_mode.add_argument(
        "--check",
        action="store_true",
        help="Exit nonzero if slices/INDEX.md differs from generated output.",
    )
    sync_parser.set_defaults(func=cmd_sync_index)

    return parser


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    _apply_overrides(args.repo, args.slices_dir)
    try:
        return args.func(args)
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
