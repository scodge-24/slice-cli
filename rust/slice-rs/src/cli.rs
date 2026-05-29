use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::Result;
use crate::commands;
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
        json: bool,
    },

    /// List all stale docs across slices.
    StaleDocs {
        #[arg(long)]
        json: bool,
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

fn run_inner(args: Args) -> Result<i32> {
    let ctx = Context::new(args.repo, args.slices_dir)?;
    match args.command {
        Command::List { json } => commands::list(&ctx, json),
        Command::Show { selector, json } => commands::show(&ctx, &selector, json),
        Command::Files { selector, json } => commands::files(&ctx, &selector, json),
        Command::Deps {
            selector,
            reverse,
            transitive,
            json,
        } => commands::deps(&ctx, &selector, reverse, transitive, json),
        Command::ForPath { path, json } => commands::for_path(&ctx, &path, json),
        Command::AffectedDocs { paths, json } => commands::affected_docs(&ctx, &paths, json),
        Command::Context { selector, json } => commands::context(&ctx, &selector, json),
        Command::StaleDocs { json } => commands::stale_docs(&ctx, json),
    }
}
