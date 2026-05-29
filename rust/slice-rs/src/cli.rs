use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};

use crate::Result;
use crate::commands::{self, ShowMode};
use crate::context::Context;

#[derive(Debug, Parser)]
#[command(name = "slice-rs", about = "Rust prototype for slice-cli hot paths.")]
pub struct Args {
    #[arg(long, value_name = "DIR")]
    repo: Option<PathBuf>,

    #[arg(long, value_name = "DIR")]
    slices_dir: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// List all slices.
    List {
        #[arg(long)]
        json: bool,
    },

    /// Show one slice.
    Show {
        selector: String,
        #[arg(long)]
        body: bool,
        #[arg(long)]
        system: bool,
        #[arg(long)]
        call_stacks: bool,
        #[arg(long)]
        verification: bool,
        #[arg(long)]
        json: bool,
    },

    /// List files owned by a slice.
    Files {
        selector: String,
        #[arg(long)]
        json: bool,
    },

    /// Show slice dependencies.
    Deps {
        selector: String,
        #[arg(long)]
        reverse: bool,
        #[arg(long)]
        transitive: bool,
        #[arg(long)]
        json: bool,
    },

    /// Find slice owners for a file path.
    #[command(name = "for")]
    ForPath {
        path: String,
        #[arg(long)]
        json: bool,
    },

    /// Find docs affected by changed file paths.
    AffectedDocs {
        paths: Vec<String>,
        #[arg(long)]
        json: bool,
    },

    /// Resolve a file path or slice to its owning slice context.
    Context {
        selector: String,
        #[arg(long)]
        strict: bool,
        #[arg(long)]
        best_effort: bool,
        #[arg(long)]
        json: bool,
    },

    /// Search slices by keyword.
    Find {
        needle: String,
        #[arg(long)]
        json: bool,
    },

    /// Run rg within a slice's files.
    Grep {
        selector: String,
        pattern: String,
        #[arg(short = 'i', long)]
        ignore_case: bool,
        #[arg(short = 'F', long)]
        fixed_strings: bool,
    },

    /// List docs linked to a slice.
    Docs {
        selector: String,
        #[arg(long)]
        json: bool,
    },

    /// List all stale docs across slices.
    StaleDocs {
        #[arg(long)]
        json: bool,
    },

    /// Run integrity checks.
    Check {
        #[arg(long)]
        strict_index: bool,
        #[arg(long)]
        no_staleness: bool,
        #[arg(long)]
        no_staged_coverage: bool,
        #[arg(long)]
        no_doc_drift: bool,
        #[arg(long)]
        require_verification: bool,
        #[arg(long)]
        json: bool,
    },

    /// Regenerate INDEX.md from frontmatter.
    SyncIndex {
        #[arg(long)]
        stdout: bool,
        #[arg(long)]
        check: bool,
    },

    /// Mark docs verified.
    Stamp {
        doc_id: Option<String>,
        #[arg(long = "slice")]
        slice_id: Option<String>,
        #[arg(long)]
        doc: Option<String>,
        #[arg(long = "all")]
        stamp_all: bool,
    },

    /// Scan a vault directory and generate DOCS.yaml.
    DocsBootstrap {
        vault_dir: PathBuf,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        force: bool,
    },

    /// Wire slice-cli into this repo.
    Init {
        #[arg(long)]
        hook: bool,
        #[arg(long)]
        ci: bool,
        #[arg(long)]
        agent: bool,
        #[arg(long = "global")]
        global_: bool,
        #[arg(long)]
        dry_run: bool,
    },
}

pub fn run() -> anyhow::Result<i32> {
    let args = Args::parse();
    match run_inner(args) {
        Ok(code) => Ok(code),
        Err(err) => {
            eprintln!("{err}");
            Ok(2)
        }
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "single clap dispatch keeps CLI mapping explicit"
)]
fn run_inner(args: Args) -> Result<i32> {
    let ctx = Context::new(args.repo, args.slices_dir)?;
    match args.command {
        Command::List { json } => commands::list(&ctx, json),
        Command::Show {
            selector,
            body,
            system,
            call_stacks,
            verification,
            json,
        } => {
            let mode = match (body, system, call_stacks, verification) {
                (false, false, false, false) => ShowMode::Metadata,
                (true, false, false, false) => ShowMode::Body,
                (false, true, false, false) => ShowMode::System,
                (false, false, true, false) => ShowMode::CallStacks,
                (false, false, false, true) => ShowMode::Verification,
                _ => {
                    return Err(crate::Error::InvalidInput(
                        "--body, --system, --call-stacks, and --verification are mutually exclusive"
                            .to_owned(),
                    ));
                }
            };
            commands::show(&ctx, &selector, mode, json)
        }
        Command::Files { selector, json } => commands::files(&ctx, &selector, json),
        Command::Deps {
            selector,
            reverse,
            transitive,
            json,
        } => commands::deps(&ctx, &selector, reverse, transitive, json),
        Command::ForPath { path, json } => commands::for_path(&ctx, &path, json),
        Command::AffectedDocs { paths, json } => commands::affected_docs(&ctx, &paths, json),
        Command::Context {
            selector,
            strict,
            best_effort,
            json,
        } => commands::context(&ctx, &selector, strict, best_effort, json),
        Command::Find { needle, json } => commands::find(&ctx, &needle, json),
        Command::Grep {
            selector,
            pattern,
            ignore_case,
            fixed_strings,
        } => commands::grep(&ctx, &selector, &pattern, ignore_case, fixed_strings),
        Command::Docs { selector, json } => commands::docs(&ctx, &selector, json),
        Command::StaleDocs { json } => commands::stale_docs(&ctx, json),
        Command::Check {
            strict_index,
            no_staleness,
            no_staged_coverage,
            no_doc_drift,
            require_verification,
            json,
        } => commands::python_fallback(
            &ctx,
            &args_with_flags(
                "check",
                &[
                    ("--strict-index", strict_index),
                    ("--no-staleness", no_staleness),
                    ("--no-staged-coverage", no_staged_coverage),
                    ("--no-doc-drift", no_doc_drift),
                    ("--require-verification", require_verification),
                    ("--json", json),
                ],
            ),
        ),
        Command::SyncIndex { stdout, check } => {
            commands::python_fallback(&ctx, &sync_index_args(stdout, check))
        }
        Command::Stamp {
            doc_id,
            slice_id,
            doc,
            stamp_all,
        } => commands::python_fallback(&ctx, &stamp_args(doc_id, slice_id, doc, stamp_all)),
        Command::DocsBootstrap {
            vault_dir,
            dry_run,
            force,
        } => commands::python_fallback(&ctx, &docs_bootstrap_args(&vault_dir, dry_run, force)),
        Command::Init {
            hook,
            ci,
            agent,
            global_,
            dry_run,
        } => commands::python_fallback(
            &ctx,
            &args_with_flags(
                "init",
                &[
                    ("--hook", hook),
                    ("--ci", ci),
                    ("--agent", agent),
                    ("--global", global_),
                    ("--dry-run", dry_run),
                ],
            ),
        ),
    }
}

fn args_with_flags(command: &str, flags: &[(&str, bool)]) -> Vec<String> {
    let mut args = vec![command.to_owned()];
    for (flag, enabled) in flags {
        push_flag(&mut args, *enabled, flag);
    }
    args
}

fn sync_index_args(stdout: bool, check: bool) -> Vec<String> {
    let mut args = vec!["sync-index".to_owned()];
    push_flag(&mut args, stdout, "--stdout");
    push_flag(&mut args, check, "--check");
    args
}

fn stamp_args(
    doc_id: Option<String>,
    slice_id: Option<String>,
    doc: Option<String>,
    stamp_all: bool,
) -> Vec<String> {
    let mut args = vec!["stamp".to_owned()];
    if let Some(doc_id) = doc_id {
        args.push(doc_id);
    }
    if let Some(slice_id) = slice_id {
        args.extend(["--slice".to_owned(), slice_id]);
    }
    if let Some(doc) = doc {
        args.extend(["--doc".to_owned(), doc]);
    }
    push_flag(&mut args, stamp_all, "--all");
    args
}

fn docs_bootstrap_args(vault_dir: &Path, dry_run: bool, force: bool) -> Vec<String> {
    let mut args = vec![
        "docs-bootstrap".to_owned(),
        vault_dir.to_string_lossy().into_owned(),
    ];
    push_flag(&mut args, dry_run, "--dry-run");
    push_flag(&mut args, force, "--force");
    args
}

fn push_flag(args: &mut Vec<String>, enabled: bool, flag: &str) {
    if enabled {
        args.push(flag.to_owned());
    }
}
