from __future__ import annotations

import argparse
import sys

import yaml

from .bootstrap import cmd_docs_bootstrap
from .commands import (cmd_affected_docs, cmd_check, cmd_context, cmd_deps,
                       cmd_docs, cmd_files, cmd_find, cmd_for, cmd_grep,
                       cmd_list, cmd_show, cmd_stale_docs, cmd_stamp,
                       cmd_sync_index)
from .context import Ctx
from .init import cmd_init


def _add_json(parser: argparse.ArgumentParser) -> None:
    parser.add_argument("--json", action="store_true", help="Emit JSON.")


def _add_selector(parser: argparse.ArgumentParser) -> None:
    parser.add_argument("selector", help="Slice ID or doc stem.")


class RichHelpFormatter(argparse.RawDescriptionHelpFormatter):
    """Preserve multi-line help text and examples."""


def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(
        prog="slice",
        description="Navigate codebase slice documents.",
        formatter_class=RichHelpFormatter,
    )
    p.add_argument("--repo", metavar="DIR", help="Override repo root.")
    p.add_argument("--slices-dir", metavar="DIR", help="Override slices directory.")
    sub = p.add_subparsers(dest="command", required=True)

    # list
    sp = sub.add_parser("list", help="List all slices.")
    _add_json(sp)
    sp.set_defaults(func=cmd_list)

    # show
    sp = sub.add_parser(
        "show", help="Show one slice (metadata, or body sections with flags).",
        formatter_class=RichHelpFormatter,
        epilog=(
            "examples:\n"
            "  slice show auth-service\n"
            "  slice show auth-service --body\n"
            "  slice show auth-service --system\n"
            "  slice show auth-service --call-stacks\n"
            "  slice show auth-service --verification"
        ),
    )
    _add_selector(sp)
    sec = sp.add_mutually_exclusive_group()
    sec.add_argument("--body", action="store_true", help="Print the full Markdown body.")
    sec.add_argument("--system", action="store_true",
                     help="Print all standard system sections.")
    sec.add_argument("--call-stacks", action="store_true", dest="call_stacks",
                     help="Print the Runtime Flows section only.")
    sec.add_argument("--verification", action="store_true",
                     help="Print the Verification and Update Triggers sections.")
    _add_json(sp)
    sp.set_defaults(func=cmd_show, body=False, system=False,
                    call_stacks=False, verification=False)

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

    # context
    sp = sub.add_parser(
        "context",
        help="Resolve a file path or slice to its owning slice + system context.",
        formatter_class=RichHelpFormatter,
        epilog=(
            "examples:\n"
            "  slice context src/auth/middleware.py\n"
            "  slice context auth-service --json\n"
            "  slice context src/auth/middleware.py --best-effort\n"
            "\n"
            "for a single body section, use slice show:\n"
            "  slice show auth-service --call-stacks\n"
            "\n"
            "ambiguity: config slices/config.yaml -> context.ambiguity "
            "(strict | best_effort);\ndefault strict. Override with --strict / --best-effort."
        ),
    )
    sp.add_argument("selector", help="Repo-relative/absolute file path, or a slice id/doc stem.")
    amb = sp.add_mutually_exclusive_group()
    amb.add_argument("--strict", action="store_true",
                     help="Fail on multiple owning slices (overrides config).")
    amb.add_argument("--best-effort", action="store_true", dest="best_effort",
                     help="Print all owning slices on ambiguity (overrides config).")
    _add_json(sp)
    sp.set_defaults(func=cmd_context, strict=False, best_effort=False)

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
    sp.add_argument("--require-verification", action="store_true",
                    help="Warn on abstractions with no verification link (V-model coverage gap).")
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
    sp = sub.add_parser(
        "stale-docs", help="List all stale docs across slices.",
        formatter_class=RichHelpFormatter,
        epilog=(
            "exit codes:\n"
            "  0  all tracked docs are current\n"
            "  1  one or more docs are stale (a status signal, not an error) —\n"
            "     useful as a pre-commit/CI gate\n"
            "\n"
            "examples:\n"
            "  slice stale-docs\n"
            "  slice stale-docs --json"
        ),
    )
    _add_json(sp)
    sp.set_defaults(func=cmd_stale_docs)

    # stamp
    sp = sub.add_parser(
        "stamp",
        help="Mark docs verified: record a content fingerprint of their tracked sources.",
        formatter_class=RichHelpFormatter,
        epilog=(
            "Records a content fingerprint of each doc's tracked files (the staleness\n"
            "anchor) plus the HEAD short-SHA as a note. Works on a dirty tree.\n"
            "\n"
            "examples:\n"
            "  slice stamp auth-guide        # stamp one doc by id\n"
            "  slice stamp --slice auth-service\n"
            "  slice stamp                   # stamp every currently-stale doc"
        ),
    )
    sp.add_argument("doc_id", nargs="?", default=None, help="Stamp a specific doc by doc_id.")
    sp.add_argument("--slice", metavar="SLICE_ID", help="Stamp all docs for a slice.")
    sp.add_argument("--doc", metavar="PATH", help="Stamp a specific doc by vault-relative path.")
    sp.add_argument("--all", action="store_true", default=False, dest="stamp_all",
                    help="Stamp all docs regardless of staleness.")
    sp.set_defaults(func=cmd_stamp)

    # affected-docs
    sp = sub.add_parser(
        "affected-docs", help="Find docs affected by changed file paths.",
        formatter_class=RichHelpFormatter,
        epilog=(
            "examples:\n"
            "  slice affected-docs src/auth/middleware.py\n"
            "  slice affected-docs $(git diff --name-only) --json\n"
            "\n"
            "exit codes: 0 = no affected docs stale, 1 = one or more affected docs stale."
        ),
    )
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

    # init
    sp = sub.add_parser(
        "init",
        help="Wire slice-cli into this repo (agent instructions, optional hook/CI).",
        formatter_class=RichHelpFormatter,
        epilog=(
            "examples:\n"
            "  slice init                 # add the agent-instruction block to CLAUDE.md/AGENTS.md\n"
            "  slice init --hook          # also install a pre-commit staleness reminder\n"
            "  slice init --hook --ci     # also add a GitHub Actions staleness check\n"
            "  slice init --agent         # also install the slice-codebase skill + codebase-slicer agent\n"
            "  slice init --agent --global  # install the skill + agent into ~/.claude (every repo)\n"
            "  slice init --dry-run       # preview without writing\n"
            "\n"
            "The agent block is wrapped in <!-- slice-cli:start/end --> markers and is\n"
            "idempotent — re-running updates it in place. --agent writes the slicing skill\n"
            "and agent into .claude/ (or ~/.claude with --global); for the managed\n"
            "alternative, install the slice-cli plugin instead."
        ),
    )
    sp.add_argument("--hook", action="store_true",
                    help="Install a git pre-commit hook that warns about stale docs.")
    sp.add_argument("--ci", action="store_true",
                    help="Write a GitHub Actions workflow running `slice check`.")
    sp.add_argument("--agent", action="store_true",
                    help="Install the slice-codebase skill + codebase-slicer agent into .claude/.")
    sp.add_argument("--global", action="store_true", dest="global_",
                    help="Write to your user-level ~/.claude instead of the repo (block, skill, agent).")
    sp.add_argument("--dry-run", action="store_true",
                    help="Print what would be written without writing.")
    sp.set_defaults(func=cmd_init)

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
    except (ValueError, RuntimeError) as exc:
        print(str(exc), file=sys.stderr)
        return 2
    except yaml.YAMLError as exc:
        # Backstop — load sites wrap safe_load via _parse_yaml, but guard anyway.
        print(f"error: malformed YAML: {str(exc).splitlines()[0]}", file=sys.stderr)
        return 2
    except FileNotFoundError as exc:
        if exc.filename == "git":
            print("error: git not found on PATH — slice requires git. "
                  "Install git and retry.", file=sys.stderr)
        else:
            print(f"error: file not found: {exc}", file=sys.stderr)
        return 2
    except OSError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2
