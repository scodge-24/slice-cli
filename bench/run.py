#!/usr/bin/env python3
"""Scale benchmark for slice-cli.

Synthesizes a throwaway git repo with N slices (each owning K files) and N
tracked docs, then times the hot commands. Produces real numbers to inform the
"is Python fast enough, or should this be ported to Rust/TS?" decision
(see design/platform-evaluation.md).

Usage:
    python bench/run.py [--slices N] [--files-per-slice K] [--repeat R]
"""
from __future__ import annotations

import argparse
import subprocess
import sys
import tempfile
import time
from pathlib import Path

CLI = str(Path(__file__).resolve().parent.parent / "slices_cli.py")


def _run(repo: Path, *cli_args: str) -> None:
    subprocess.run([sys.executable, CLI, "--repo", str(repo), *cli_args],
                   capture_output=True, check=False)


def build_repo(root: Path, n_slices: int, files_per_slice: int) -> None:
    subprocess.run(["git", "init", "-q"], cwd=root, check=True)
    subprocess.run(["git", "config", "user.email", "b@b"], cwd=root, check=True)
    subprocess.run(["git", "config", "user.name", "b"], cwd=root, check=True)
    src = root / "src"
    slices = root / "slices"
    docs = root / "docs"
    for d in (src, slices, docs):
        d.mkdir()

    docs_section: list[str] = ["vault_root: ../docs", "docs:"]
    index_rows: list[str] = []
    for i in range(n_slices):
        sid = f"slice-{i:04d}"
        files = []
        for k in range(files_per_slice):
            fp = src / f"{sid}" / f"mod_{k}.py"
            fp.parent.mkdir(parents=True, exist_ok=True)
            fp.write_text(f"def f_{i}_{k}():\n    return {i * k}\n")
            files.append(f"src/{sid}/mod_{k}.py")
        files_yaml = "\n".join(f"  - {f}" for f in files)
        (slices / f"{sid}.md").write_text(
            f"---\nslice_id: {sid}\ndescription: Slice {i}\nloc: {files_per_slice * 2}\n"
            f"files:\n{files_yaml}\ndependencies: []\n---\n## Runtime Flows\nstep\n"
        )
        (docs / f"doc-{i:04d}.md").write_text(f"---\ndoc_id: doc-{i:04d}\n---\n# Doc {i}\n")
        index_rows.append(f"| `{sid}` | Slice {i} | ~{files_per_slice * 2} |")
        docs_section += [
            f"  doc-{i:04d}:",
            f"    path: doc-{i:04d}.md",
            f"    slices: [{sid}]",
            "    verified_at: \"\"",
        ]

    (slices / "INDEX.md").write_text(
        "# Slice Index\n\nLast updated: 0\n\n| Slice ID | Description | LoC |\n"
        "|---|---|---|\n" + "\n".join(index_rows) + "\n"
    )
    (slices / "DOCS.yaml").write_text("\n".join(docs_section) + "\n")
    subprocess.run(["git", "add", "-A"], cwd=root, check=True)
    subprocess.run(["git", "commit", "-q", "-m", "bench"], cwd=root, check=True)
    # Stamp everything so staleness has real fingerprints to compare.
    _run(root, "stamp", "--all")


def time_cmd(repo: Path, args: list[str], repeat: int) -> float:
    best = float("inf")
    for _ in range(repeat):
        t0 = time.perf_counter()
        _run(repo, *args)
        best = min(best, time.perf_counter() - t0)
    return best


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--slices", type=int, default=200)
    ap.add_argument("--files-per-slice", type=int, default=3)
    ap.add_argument("--repeat", type=int, default=3)
    args = ap.parse_args()

    with tempfile.TemporaryDirectory() as tmp:
        repo = Path(tmp)
        print(f"building repo: {args.slices} slices x {args.files_per_slice} files...")
        build_repo(repo, args.slices, args.files_per_slice)
        total_files = args.slices * args.files_per_slice
        print(f"repo ready: {args.slices} slices, {args.slices} docs, {total_files} source files\n")
        print(f"{'command':<32}{'best of ' + str(args.repeat):>12}")
        print("-" * 44)
        for label, cli_args in [
            ("check", ["check"]),
            ("stale-docs", ["stale-docs"]),
            ("affected-docs (1 file)", ["affected-docs", "src/slice-0000/mod_0.py"]),
            ("list", ["list"]),
        ]:
            secs = time_cmd(repo, cli_args, args.repeat)
            print(f"{label:<32}{secs * 1000:>9.0f} ms")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
