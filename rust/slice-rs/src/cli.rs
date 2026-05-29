use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::Result;
use crate::check::{CheckOptions, output as check_output, run as run_check};
use crate::commands::{self, ShowMode};
use crate::context::Context;
use crate::index;

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
        } => {
            let options = CheckOptions {
                strict_index,
                staleness: !no_staleness,
                staged_coverage: !no_staged_coverage,
                doc_drift: !no_doc_drift,
                require_verification,
            };
            let (slice_count, result) = run_check(&ctx, options)?;
            let ok = result.ok();
            if json {
                commands::emit_json(&check_output(slice_count, result, strict_index))?;
            } else {
                let status = if ok { "OK" } else { "FAILED" };
                println!("{status}: checked {slice_count} slices");
                if !result.errors.is_empty() {
                    println!("Errors:");
                    for error in &result.errors {
                        println!("  - {error}");
                    }
                }
                if !result.warnings.is_empty() {
                    println!("Warnings:");
                    for warning in &result.warnings {
                        println!("  - {warning}");
                    }
                } else if ok {
                    println!("Warnings: none");
                }
                if !result.hidden_warnings.is_empty() && !strict_index {
                    println!(
                        "({} index drift warnings hidden - use --strict-index to show)",
                        result.hidden_warnings.len()
                    );
                }
            }
            Ok(i32::from(!ok))
        }
        Command::SyncIndex { stdout, check } => index::sync_index(&ctx, stdout, check),
        Command::Stamp {
            doc_id,
            slice_id,
            doc,
            stamp_all,
        } => commands::stamp(
            &ctx,
            doc_id.as_deref(),
            slice_id.as_deref(),
            doc.as_deref(),
            stamp_all,
        ),
        Command::DocsBootstrap {
            vault_dir,
            dry_run,
            force,
        } => commands::docs_bootstrap(&ctx, &vault_dir, dry_run, force),
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

fn push_flag(args: &mut Vec<String>, enabled: bool, flag: &str) {
    if enabled {
        args.push(flag.to_owned());
    }
}
