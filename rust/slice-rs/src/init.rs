use std::path::{Path, PathBuf};

use crate::context::Context;
use crate::{Error, Result};

const BLOCK_START: &str = "<!-- slice-cli:start -->";
const BLOCK_END: &str = "<!-- slice-cli:end -->";

const AGENT_INSTRUCTIONS: &str = "\
## slice-cli

`slice` (slice-cli) turns this repo's `slices/*.md` cards into a fast query surface for
navigating code - ownership, blast radius, call stacks, concepts - plus tracking whether
design docs have gone stale. Reach for it before editing unfamiliar code.

Navigate:

- `slice context <path>` - orient on a file: its owning slice, dependencies, runtime
  flows, and system notes (add `--json` for linked docs with stale status).
- `slice for <path>` - which slice owns a path.
- `slice show <id> --call-stacks` / `--system` - runtime call chains / full system notes.
- `slice deps <id> --reverse --transitive` - blast radius: every slice that
  (transitively) depends on this one, before you change it.
- `slice find <needle>` - locate a concept or abstraction across slices.

Track doc staleness:

- After changing source, run `slice affected-docs <changed-files>` to see which docs may
  be stale. Update them, then `slice stamp <doc-id>` to mark verified. `slice stale-docs`
  lists everything stale (exit 1 if any).

If `slices/` is missing or out of date, run `/slice-codebase` to (re)generate it.
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
#[derive(Debug, Clone)]
pub struct InitOptions {
    pub hook: bool,
    pub ci: bool,
    pub agent: bool,
    pub global: bool,
    pub dry_run: bool,
    pub docs: Option<PathBuf>,
}

pub fn run(ctx: &Context, options: &InitOptions) -> Result<i32> {
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
        if let Some(docs_dir) = options.docs.as_deref() {
            crate::commands::setup_docs_manifest(ctx, docs_dir, true)?;
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
    if let Some(docs_dir) = options.docs.as_deref() {
        crate::commands::setup_docs_manifest(ctx, docs_dir, false)?;
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

#[cfg(test)]
mod tests {
    use super::{CODEBASE_SLICER_AGENT, SLICE_CODEBASE_SKILL, render_agent_block, upsert_block};

    #[test]
    fn embedded_templates_match_committed_files() {
        assert_eq!(
            SLICE_CODEBASE_SKILL,
            include_str!("../../../skills/slice-codebase/SKILL.md")
        );
        assert_eq!(
            CODEBASE_SLICER_AGENT,
            include_str!("../../../agents/codebase-slicer.md")
        );
    }

    #[test]
    fn upsert_preserves_existing_content_and_replaces_owned_block() {
        let first = upsert_block("# Existing\n", &render_agent_block());
        assert!(first.starts_with("# Existing\n\n<!-- slice-cli:start -->"));
        let second = upsert_block(&first, &render_agent_block());
        assert_eq!(second.matches("<!-- slice-cli:start -->").count(), 1);
    }
}
