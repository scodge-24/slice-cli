use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::Result;
use crate::check::{CheckOptions, output as check_output, run as run_check};
use crate::color::{ColorChoice, Styles};
use crate::commands::{self, ShowMode};
use crate::context::Context;
use crate::index;

#[derive(Debug, Parser)]
#[command(
    name = "slice",
    about = "Navigate your codebase by slice: ownership, blast radius, call stacks, concepts, and doc staleness."
)]
pub struct Args {
    #[arg(long, value_name = "DIR")]
    repo: Option<PathBuf>,

    #[arg(long, value_name = "DIR")]
    slices_dir: Option<PathBuf>,

    /// When to colorize human output (never affects --json).
    #[arg(long, global = true, default_value = "auto", value_name = "WHEN")]
    color: ColorChoice,

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
    #[command(after_help = "Section flags: --body, --system, --call-stacks, and --verification.")]
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

    /// Outline a file's symbols (definitions + line spans).
    #[command(
        after_help = "Each row is file:start-end\\tname; the trailing `coverage: N/M definitions \
                      spanned` declares how many detected definitions the heuristic could confidently \
                      span — ambiguous cases (decorators, nested functions, multi-line signatures, \
                      tab indent) are skipped, not guessed, so a low ratio is a real signal, not noise."
    )]
    Outline {
        /// Source file to outline (any tracked file; slice membership not required).
        file: String,
        #[arg(long)]
        json: bool,
    },

    /// List the symbols defined across a slice's files (with declared coverage).
    Symbols {
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
        /// List the concrete files of the dependency slices (the blast radius), not just slice ids.
        #[arg(long)]
        files: bool,
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
    #[command(
        after_help = "Examples:\n  slice context src/auth/middleware.py\n  slice context auth-service --json"
    )]
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
    ///
    /// With --semantic (requires the `semantic` build feature + a built index), ranks slices by
    /// embedding similarity instead of keyword match — for concepts you can't name by symbol.
    Find {
        needle: String,
        #[arg(long)]
        json: bool,
        /// Semantic (embedding) search over the slice index instead of keyword match.
        #[cfg(feature = "semantic")]
        #[arg(long)]
        semantic: bool,
    },

    /// Fuzzy-pick a slice with fzf (interactive).
    ///
    /// Default: show the chosen slice. With --print: emit its id for piping,
    /// e.g. `id=$(slice browse --print) && slice show "$id"`.
    #[command(after_help = "Requires fzf >= 0.30 on PATH.")]
    Browse {
        /// Initial fuzzy query.
        #[arg(short = 'q', long)]
        query: Option<String>,
        /// Print the selected slice id instead of showing the slice.
        #[arg(long)]
        print: bool,
    },

    /// Run rg within a slice's files.
    #[command(
        after_help = "With --symbols, each hit gets a trailing \\t[span <Name> <start>-<end> approx] \
                      naming the enclosing function/class. Spans are a best-effort heuristic \
                      (`approx`); ambiguous cases (decorators, nested functions, multi-line \
                      signatures, tab indent) are left unannotated rather than guessed."
    )]
    Grep {
        selector: String,
        pattern: String,
        #[arg(short = 'i', long)]
        ignore_case: bool,
        #[arg(short = 'F', long)]
        fixed_strings: bool,
        /// Annotate each hit with its enclosing symbol's line span (heuristic; opt-in).
        #[arg(long)]
        symbols: bool,
    },

    /// List docs linked to a slice.
    Docs {
        selector: String,
        #[arg(long)]
        json: bool,
    },

    /// List all stale docs across slices.
    ///
    /// Exits 0 when all tracked docs are current, 1 when any doc is stale.
    #[command(
        after_help = "Exit codes: 0 when all docs are current; exit 1 when any doc is stale."
    )]
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

    /// Scan a documentation directory and generate DOCS.yaml.
    ///
    /// Bootstraps doc→slice mappings from docs whose frontmatter carries
    /// `tracks:`, or writes a commented stub seeded with the docs it finds.
    #[command(
        after_help = "Examples:\n  slice docs-bootstrap docs\n  slice docs-bootstrap docs --force"
    )]
    DocsBootstrap {
        docs_dir: PathBuf,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        force: bool,
    },

    /// Build the semantic embedding index over slice-anchored units (writes slices/SEMANTIC.json).
    ///
    /// Requires the `semantic` build feature and an `OPENROUTER_API_KEY`. The index is slice-owned,
    /// regenerable state; rebuild it after slices change.
    #[cfg(feature = "semantic")]
    SemanticIndex {
        /// Embedding model (default: google/gemini-embedding-2).
        #[arg(long)]
        model: Option<String>,
        /// Embedding dimensions (default: 512; only models supporting reduction honour it).
        #[arg(long)]
        dimensions: Option<usize>,
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
    let color = args.color;
    let styles = Styles::resolve(color);
    match args.command {
        Command::List { json } => commands::list(&ctx, json, &styles),
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
            commands::show(&ctx, &selector, mode, json, &styles)
        }
        Command::Files { selector, json } => commands::files(&ctx, &selector, json),
        Command::Outline { file, json } => commands::outline(&ctx, &file, json),
        Command::Symbols { selector, json } => commands::symbols(&ctx, &selector, json),
        Command::Deps {
            selector,
            reverse,
            transitive,
            files,
            json,
        } => commands::deps(&ctx, &selector, reverse, transitive, files, json),
        Command::ForPath { path, json } => commands::for_path(&ctx, &path, json),
        Command::AffectedDocs { paths, json } => commands::affected_docs(&ctx, &paths, json),
        Command::Context {
            selector,
            strict,
            best_effort,
            json,
        } => commands::context(&ctx, &selector, strict, best_effort, json),
        Command::Find {
            needle,
            json,
            #[cfg(feature = "semantic")]
            semantic,
        } => {
            #[cfg(feature = "semantic")]
            if semantic {
                return crate::semantic::query(&ctx, &needle, json, &styles);
            }
            commands::find(&ctx, &needle, json, &styles)
        }
        Command::Browse { query, print } => {
            commands::browse(&ctx, query.as_deref(), print, &styles, color)
        }
        Command::Grep {
            selector,
            pattern,
            ignore_case,
            fixed_strings,
            symbols,
        } => commands::grep(
            &ctx,
            &selector,
            &pattern,
            ignore_case,
            fixed_strings,
            symbols,
        ),
        Command::Docs { selector, json } => commands::docs(&ctx, &selector, json),
        Command::StaleDocs { json } => commands::stale_docs(&ctx, json, &styles),
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
            docs_dir,
            dry_run,
            force,
        } => commands::docs_bootstrap(&ctx, &docs_dir, dry_run, force),
        #[cfg(feature = "semantic")]
        Command::SemanticIndex { model, dimensions } => {
            crate::semantic::build_index(&ctx, model, dimensions)
        }
    }
}
