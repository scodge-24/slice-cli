use std::path::{Path, PathBuf};

use crate::context::Context;
use crate::{Error, Result};

const BLOCK_START: &str = "<!-- slice-cli:start -->";
const BLOCK_END: &str = "<!-- slice-cli:end -->";

const AGENT_INSTRUCTIONS: &str = "\
## slice-cli

This repo tracks whether design docs are stale relative to the code they
describe, via `slice` (slice-cli) and `slices/DOCS.yaml`.

- Before editing an unfamiliar file, run `slice context <path>` to see the
  owning slice, its system context, and any stale linked docs.
- After changing source, run `slice affected-docs <changed-files>` to see which
  docs may need updating. Update stale docs, then `slice stamp <doc-id>` to mark
  them verified.
- `slice stale-docs` lists everything currently stale (exit 1 if any are stale).
- If `slices/` is missing or out of date, run `/slice-codebase` to (re)generate
  slice definitions.
";

const HOOK_SCRIPT: &str = "\
#!/bin/sh
# Installed by `slice init --hook`. Warns about stale docs; never blocks a commit.
if command -v slice >/dev/null 2>&1; then
    if ! slice stale-docs >/dev/null 2>&1; then
        echo \"slice-cli: some tracked docs are stale - run 'slice stale-docs' to review.\" >&2
    fi
fi
exit 0
";

const CI_WORKFLOW: &str = "\
name: slice staleness
on: [push, pull_request]
jobs:
  staleness:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0
      - uses: actions/setup-python@v5
        with:
          python-version: \"3.12\"
      # Install the slice CLI. Swap for a pinned version or a git source as needed:
      #   pip install slice-cli==0.1.0
      #   pip install git+https://github.com/scodge-24/slice-cli
      - run: pip install slice-cli
      - run: slice check
";

const SLICE_CODEBASE_SKILL: &str = include_str!("../../../skills/slice-codebase/SKILL.md");
const CODEBASE_SLICER_AGENT: &str = include_str!("../../../agents/codebase-slicer.md");

#[expect(clippy::struct_excessive_bools, reason = "mirrors CLI init flags")]
#[derive(Debug, Clone, Copy)]
pub struct InitOptions {
    pub hook: bool,
    pub ci: bool,
    pub agent: bool,
    pub global: bool,
    pub dry_run: bool,
}

pub fn run(ctx: &Context, options: InitOptions) -> Result<i32> {
    let mut planned = Vec::new();
    let block = render_agent_block();
    let mut agent_files = vec![ctx.repo_root().join("CLAUDE.md")];
    if ctx.repo_root().join("AGENTS.md").exists() {
        agent_files.push(ctx.repo_root().join("AGENTS.md"));
    }
    if options.global {
        agent_files = vec![home_dir().join(".claude/CLAUDE.md")];
    }
    for path in agent_files {
        let existing = std::fs::read_to_string(&path).unwrap_or_default();
        planned.push((path, upsert_block(&existing, &block)));
    }

    if options.hook {
        planned.push((
            ctx.repo_root().join(".git/hooks/pre-commit"),
            HOOK_SCRIPT.to_owned(),
        ));
    }
    if options.ci {
        planned.push((
            ctx.repo_root()
                .join(".github/workflows/slice-staleness.yml"),
            CI_WORKFLOW.to_owned(),
        ));
    }
    if options.agent {
        let base = if options.global {
            home_dir()
        } else {
            ctx.repo_root().to_path_buf()
        };
        let loose_skill =
            SLICE_CODEBASE_SKILL.replace("slice-cli:codebase-slicer", "codebase-slicer");
        planned.push((
            base.join(".claude/skills/slice-codebase/SKILL.md"),
            loose_skill,
        ));
        planned.push((
            base.join(".claude/agents/codebase-slicer.md"),
            CODEBASE_SLICER_AGENT.to_owned(),
        ));
    }

    if options.dry_run {
        for (path, _) in planned {
            println!("would write: {}", ctx.rel(&path));
        }
        return Ok(0);
    }

    for (path, content) in planned {
        write_file(&path, &content)?;
        if path.file_name().is_some_and(|name| name == "pre-commit") {
            make_executable(&path)?;
        }
        println!("wrote {}", ctx.rel(&path));
    }
    Ok(0)
}

fn render_agent_block() -> String {
    format!("{BLOCK_START}\n{AGENT_INSTRUCTIONS}{BLOCK_END}\n")
}

fn upsert_block(existing: &str, block: &str) -> String {
    if let (Some(start), Some(end_start)) = (existing.find(BLOCK_START), existing.find(BLOCK_END)) {
        let end = end_start + BLOCK_END.len();
        let tail = existing[end..]
            .strip_prefix('\n')
            .unwrap_or(&existing[end..]);
        return format!("{}{}{}", &existing[..start], block, tail);
    }
    let separator = if existing.is_empty() || existing.ends_with("\n\n") {
        ""
    } else if existing.ends_with('\n') {
        "\n"
    } else {
        "\n\n"
    };
    format!("{existing}{separator}{block}")
}

fn write_file(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| Error::Write {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    std::fs::write(path, content).map_err(|source| Error::Write {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let permissions = std::fs::Permissions::from_mode(0o755);
    std::fs::set_permissions(path, permissions).map_err(|source| Error::Write {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<()> {
    Ok(())
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME").map_or_else(PathBuf::new, PathBuf::from)
}
