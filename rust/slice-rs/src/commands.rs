use std::collections::{BTreeMap, VecDeque};
use std::io::{self, Write};
use std::path::Path;
use std::process::Command;

use rustc_hash::{FxHashMap, FxHashSet};
use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::config::{Ambiguity, load_ambiguity};
use crate::context::Context;
use crate::manifest::{load_doc_manifest, save_doc_manifest};
use crate::models::{
    AffectedDoc, ContextDoc, ContextOutput, ContextSlice, DepsOutput, FindMatch, ListRow,
    ShowSlice, SliceDoc, SliceDocStatus, SliceOwner, StaleDoc, TrackedDoc, TrackedDocSummary,
};
use crate::paths::{expand_literal_or_existing, matches_path, repo_join};
use crate::slices::{docs_for_slice, load_slice_docs, owners_for_path, slice_for_selector};
use crate::{Error, Result};

const STANDARD_SECTIONS: [&str; 5] = [
    "System Behavior",
    "Invariants",
    "Runtime Flows",
    "Verification",
    "Update Triggers",
];

#[derive(Debug, Clone, Copy)]
pub enum ShowMode {
    Metadata,
    Body,
    System,
    CallStacks,
    Verification,
}

pub fn list(ctx: &Context, json: bool) -> Result<i32> {
    let docs = load_slice_docs(ctx)?;
    let manifest = load_doc_manifest(ctx)?;
    if json {
        let rows = docs
            .iter()
            .map(|doc| ListRow {
                description: &doc.description,
                doc_count: docs_for_slice(&manifest.docs, &doc.slice_id).len(),
                loc: doc.loc,
                slice_id: &doc.slice_id,
            })
            .collect::<Vec<_>>();
        emit_json(&rows)?;
    } else {
        let width = docs
            .iter()
            .map(|doc| doc.slice_id.len())
            .max()
            .unwrap_or(10);
        for doc in docs {
            let loc = doc.loc.map_or(String::new(), |loc| format!(" ({loc} LoC)"));
            let doc_count = docs_for_slice(&manifest.docs, &doc.slice_id).len();
            let doc_label = if doc_count == 0 {
                String::new()
            } else {
                format!(" [{doc_count} docs]")
            };
            println!(
                "{:<width$}  {}{}{}",
                doc.slice_id, doc.description, loc, doc_label
            );
        }
    }
    Ok(0)
}

pub fn show(ctx: &Context, selector: &str, mode: ShowMode, json: bool) -> Result<i32> {
    let docs = load_slice_docs(ctx)?;
    let doc = require_slice(&docs, selector)?;
    if !matches!(mode, ShowMode::Metadata) {
        return show_sections(doc, mode, json);
    }
    let manifest = load_doc_manifest(ctx)?;
    let linked_docs = docs_for_slice(&manifest.docs, &doc.slice_id);
    if json {
        let output = ShowSlice {
            abstractions: &doc.abstractions,
            dependencies: &doc.dependencies,
            description: &doc.description,
            doc_path: ctx.rel(&doc.doc_path),
            docs: linked_docs
                .into_iter()
                .map(|tracked| TrackedDocSummary {
                    doc_id: &tracked.doc_id,
                    path: &tracked.path,
                    tags: &tracked.tags,
                    verified_at: &tracked.verified_at,
                })
                .collect(),
            exclusions: &doc.exclusions,
            files: &doc.files,
            loc: doc.loc,
            slice_id: &doc.slice_id,
        };
        emit_json(&output)?;
    } else {
        println!("slice_id: {}", doc.slice_id);
        println!("description: {}", doc.description);
        println!(
            "loc: {}",
            doc.loc.map_or_else(|| "null".to_owned(), |v| v.to_string())
        );
        println!("doc_path: {}", ctx.rel(&doc.doc_path));
        print_list("files", &doc.files);
        print_list("dependencies", &doc.dependencies);
        print_list("abstractions", &doc.abstractions);
        print_list("exclusions", &doc.exclusions);
    }
    Ok(0)
}

pub fn files(ctx: &Context, selector: &str, json: bool) -> Result<i32> {
    let docs = load_slice_docs(ctx)?;
    let doc = require_slice(&docs, selector)?;
    if json {
        emit_json(&doc.files)?;
    } else {
        for file in &doc.files {
            println!("{file}");
        }
    }
    Ok(0)
}

pub fn deps(
    ctx: &Context,
    selector: &str,
    reverse: bool,
    transitive: bool,
    json: bool,
) -> Result<i32> {
    let docs = load_slice_docs(ctx)?;
    let doc = require_slice(&docs, selector)?;
    let (dependencies, mode) = if reverse {
        (
            reverse_deps(&docs)
                .remove(&doc.slice_id)
                .unwrap_or_default(),
            "reverse",
        )
    } else if transitive {
        (transitive_deps(&doc.slice_id, &docs), "transitive")
    } else {
        (doc.dependencies.clone(), "direct")
    };

    if json {
        emit_json(&DepsOutput {
            dependencies,
            mode,
            slice_id: &doc.slice_id,
        })?;
    } else {
        for dep in dependencies {
            println!("{dep}");
        }
    }
    Ok(0)
}

pub fn for_path(ctx: &Context, path: &str, json: bool) -> Result<i32> {
    let docs = load_slice_docs(ctx)?;
    let owners = owners_for_path(&docs, path, ctx);
    if json {
        let output = owners
            .iter()
            .map(|owner| SliceOwner {
                description: &owner.description,
                slice_id: &owner.slice_id,
            })
            .collect::<Vec<_>>();
        emit_json(&output)?;
        return Ok(0);
    }
    if owners.is_empty() {
        eprintln!(
            "no owning slice for: {}",
            crate::paths::normalize_repo_path(path, ctx)
        );
        return Ok(1);
    }
    for owner in owners {
        println!("{}\t{}", owner.slice_id, owner.description);
    }
    Ok(0)
}

pub fn affected_docs(ctx: &Context, paths: &[String], json: bool) -> Result<i32> {
    let docs = load_slice_docs(ctx)?;
    let manifest = load_doc_manifest(ctx)?;
    let mut affected_slice_ids = FxHashSet::default();
    for path in paths {
        for owner in owners_for_path(&docs, path, ctx) {
            affected_slice_ids.insert(owner.slice_id.clone());
        }
    }

    if affected_slice_ids.is_empty() {
        if json {
            emit_json(&Vec::<AffectedDoc>::new())?;
        } else {
            println!("no owning slices found for given paths");
        }
        return Ok(0);
    }

    let affected_docs = manifest
        .docs
        .iter()
        .filter(|doc| {
            doc.slices
                .iter()
                .any(|sid| affected_slice_ids.contains(sid))
        })
        .collect::<Vec<_>>();

    if affected_docs.is_empty() {
        if json {
            emit_json(&Vec::<AffectedDoc>::new())?;
        } else {
            println!("no tracked docs for affected slices");
        }
        return Ok(0);
    }

    let stale = stale_docs_for(ctx, &docs, &manifest.docs, StalenessMode::Fast);
    let stale_by_id = stale
        .iter()
        .map(|doc| (doc.doc_id.as_str(), doc))
        .collect::<FxHashMap<_, _>>();
    let mut output = Vec::with_capacity(affected_docs.len());
    for doc in affected_docs {
        let matching_slices = doc
            .slices
            .iter()
            .filter(|sid| affected_slice_ids.contains(*sid))
            .cloned()
            .collect::<Vec<_>>();
        let stale_doc = stale_by_id.get(doc.doc_id.as_str());
        output.push(AffectedDoc {
            changed_files: stale_doc
                .map_or_else(Vec::new, |stale_doc| stale_doc.changed_files.clone()),
            doc_id: doc.doc_id.clone(),
            matching_slices,
            path: doc.path.clone(),
            status: if stale_doc.is_some() {
                "stale"
            } else {
                "current"
            }
            .to_owned(),
        });
    }

    let any_stale = output.iter().any(|doc| doc.status == "stale");
    if json {
        emit_json(&output)?;
    } else {
        for doc in &output {
            let status = if doc.status == "stale" {
                "STALE"
            } else {
                "ok   "
            };
            println!(
                "[{status}] {}  ({})  [{}]",
                doc.doc_id,
                doc.path,
                doc.matching_slices.join(", ")
            );
            for file in &doc.changed_files {
                println!("  - {file}");
            }
        }
    }
    Ok(i32::from(any_stale))
}

pub fn context(
    ctx: &Context,
    selector: &str,
    strict: bool,
    best_effort: bool,
    json: bool,
) -> Result<i32> {
    let docs = load_slice_docs(ctx)?;
    let manifest = load_doc_manifest(ctx)?;
    let ambiguity = if strict && best_effort {
        return Err(Error::InvalidInput(
            "--strict and --best-effort are mutually exclusive".to_owned(),
        ));
    } else if strict {
        Ambiguity::Strict
    } else if best_effort {
        Ambiguity::BestEffort
    } else {
        load_ambiguity(ctx)?
    };

    let mut targets = owners_for_path(&docs, selector, ctx);
    if targets.is_empty() {
        targets = if let Some(doc) = slice_for_selector(&docs, selector) {
            vec![doc]
        } else {
            eprintln!(
                "no owning slice for: {}",
                crate::paths::normalize_repo_path(selector, ctx)
            );
            return Ok(1);
        };
    } else if targets.len() > 1 && ambiguity == Ambiguity::Strict {
        targets.sort_by(|a, b| a.slice_id.cmp(&b.slice_id));
        let ids = targets
            .iter()
            .map(|doc| doc.slice_id.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        eprintln!(
            "ambiguous: multiple slices own {}: {ids}",
            crate::paths::normalize_repo_path(selector, ctx)
        );
        return Ok(1);
    }
    targets.sort_by(|a, b| a.slice_id.cmp(&b.slice_id));

    let stale_ids = stale_docs_for(ctx, &docs, &manifest.docs, StalenessMode::Fast)
        .into_iter()
        .map(|doc| doc.doc_id)
        .collect::<std::collections::BTreeSet<_>>();

    if json {
        let slices = targets
            .into_iter()
            .map(|doc| {
                let linked_docs = docs_for_slice(&manifest.docs, &doc.slice_id)
                    .into_iter()
                    .map(|tracked| ContextDoc {
                        doc_id: tracked.doc_id.clone(),
                        path: tracked.path.clone(),
                        stale: stale_ids.contains(&tracked.doc_id),
                        verified_at: tracked.verified_at.clone(),
                    })
                    .collect();
                ContextSlice {
                    dependencies: doc.dependencies.clone(),
                    description: doc.description.clone(),
                    doc_path: ctx.rel(&doc.doc_path),
                    docs: linked_docs,
                    files: doc.files.clone(),
                    sections: present_sections(&doc.body),
                    slice_id: doc.slice_id.clone(),
                }
            })
            .collect();
        emit_json(&ContextOutput { slices })?;
    } else {
        for doc in targets {
            println!("slice: {}", doc.slice_id);
            println!("description: {}", doc.description);
            println!("doc: {}", ctx.rel(&doc.doc_path));
            println!("files: {}", doc.files.join(", "));
            println!("dependencies: {}", doc.dependencies.join(", "));
            for (heading, text) in present_sections(&doc.body) {
                println!("{heading}:");
                println!("{text}");
            }
        }
    }
    Ok(0)
}

pub fn find(ctx: &Context, needle: &str, json: bool) -> Result<i32> {
    let docs = load_slice_docs(ctx)?;
    let manifest = load_doc_manifest(ctx)?;
    let matches = find_matches(&docs, &manifest.docs, needle);
    if json {
        emit_json(&matches)?;
        return Ok(i32::from(matches.is_empty()));
    }
    if matches.is_empty() {
        eprintln!("no matches for: {needle}");
        return Ok(1);
    }
    let width = matches
        .iter()
        .map(|row| row.slice_id.len())
        .max()
        .unwrap_or(0);
    for row in matches {
        println!(
            "{:<width$}  [{}]  {}",
            row.slice_id,
            row.matches.join(","),
            row.description
        );
    }
    Ok(0)
}

pub fn grep(
    ctx: &Context,
    selector: &str,
    pattern: &str,
    ignore_case: bool,
    fixed_strings: bool,
) -> Result<i32> {
    let docs = load_slice_docs(ctx)?;
    let doc = require_slice(&docs, selector)?;
    if doc.files.is_empty() {
        eprintln!("{} has no files[]", doc.slice_id);
        return Ok(1);
    }

    let mut command = Command::new("rg");
    command.arg("-n");
    if ignore_case {
        command.arg("-i");
    }
    if fixed_strings {
        command.arg("-F");
    }
    command.arg(pattern);
    for slice_pattern in &doc.files {
        let expanded = expand_literal_or_existing(slice_pattern, ctx);
        if expanded.is_empty() {
            command.arg(slice_pattern);
        } else {
            command.args(expanded);
        }
    }
    match command.current_dir(ctx.repo_root()).status() {
        Ok(status) => Ok(status.code().unwrap_or(1)),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("rg is required for `slice grep`");
            Ok(2)
        }
        Err(err) => Err(Error::Io(err)),
    }
}

pub fn docs(ctx: &Context, selector: &str, json: bool) -> Result<i32> {
    let slices = load_slice_docs(ctx)?;
    let doc = require_slice(&slices, selector)?;
    let manifest = load_doc_manifest(ctx)?;
    let linked_docs = docs_for_slice(&manifest.docs, &doc.slice_id);
    let relevant_docs = linked_docs.iter().copied().cloned().collect::<Vec<_>>();
    let stale_ids = stale_docs_for(ctx, &slices, &relevant_docs, StalenessMode::Fast)
        .into_iter()
        .map(|stale| stale.doc_id)
        .collect::<FxHashSet<_>>();

    if json {
        let rows = linked_docs
            .iter()
            .map(|tracked| SliceDocStatus {
                doc_id: &tracked.doc_id,
                path: &tracked.path,
                stale: stale_ids.contains(&tracked.doc_id),
                tags: &tracked.tags,
                verified_at: &tracked.verified_at,
            })
            .collect::<Vec<_>>();
        emit_json(&rows)?;
        return Ok(0);
    }

    if linked_docs.is_empty() {
        println!("no docs linked to slice '{}'", doc.slice_id);
        return Ok(0);
    }
    for tracked in linked_docs {
        let status = if stale_ids.contains(&tracked.doc_id) {
            "STALE"
        } else {
            "ok   "
        };
        let tags = if tracked.tags.is_empty() {
            String::new()
        } else {
            format!("  [{}]", tracked.tags.join(", "))
        };
        let verified = if tracked.verified_at.is_empty() {
            "(never)"
        } else {
            &tracked.verified_at
        };
        println!(
            "[{status}] {}  ({})  (verified: {verified}){tags}",
            tracked.doc_id, tracked.path
        );
    }
    Ok(0)
}

pub fn stale_docs(ctx: &Context, json: bool) -> Result<i32> {
    let docs = load_slice_docs(ctx)?;
    let manifest = load_doc_manifest(ctx)?;
    let stale = stale_docs_for(ctx, &docs, &manifest.docs, StalenessMode::Attributed);
    let any_stale = !stale.is_empty();
    if json {
        emit_json(&stale)?;
    } else if stale.is_empty() {
        println!("all docs are up to date");
    } else {
        for doc in &stale {
            println!(
                "{}  ({})  (since {})  [{}]",
                doc.doc_id,
                doc.path,
                doc.verified_at.chars().take(12).collect::<String>(),
                doc.affected_slices.join(", ")
            );
            for file in &doc.changed_files {
                println!("  - {file}");
            }
        }
    }
    Ok(i32::from(any_stale))
}

pub fn stamp(
    ctx: &Context,
    doc_id: Option<&str>,
    slice_id: Option<&str>,
    doc_path: Option<&str>,
    stamp_all: bool,
) -> Result<i32> {
    let manifest = load_doc_manifest(ctx)?;
    if manifest.docs.is_empty() {
        eprintln!("no DOCS.yaml manifest found");
        return Ok(2);
    }
    let head = ctx.head_sha();
    if head == "unknown" {
        eprintln!("cannot determine HEAD");
        return Ok(2);
    }
    let short_sha = head.chars().take(12).collect::<String>();
    let (targets, empty_code) =
        stamp_targets(ctx, &manifest.docs, doc_id, slice_id, doc_path, stamp_all)?;
    if targets.is_empty() {
        return Ok(empty_code);
    }

    let slices = load_slice_docs(ctx)?;
    let by_id = slices
        .iter()
        .map(|slice| (slice.slice_id.as_str(), slice))
        .collect::<FxHashMap<_, _>>();
    let target_ids = targets.into_iter().collect::<FxHashSet<_>>();
    let mut updated = Vec::with_capacity(manifest.docs.len());
    for mut doc in manifest.docs {
        if target_ids.contains(doc.doc_id.as_str()) {
            let concrete = resolve_tracked_files(&doc, &by_id)
                .iter()
                .flat_map(|file| expand_literal_or_existing(file, ctx))
                .map(|file| ctx.git_relative_path(&file))
                .collect::<Vec<_>>();
            doc.verified_at.clone_from(&short_sha);
            doc.fingerprint = content_fingerprint(ctx, &concrete);
            println!("stamped {} -> {short_sha}", doc.doc_id);
        }
        updated.push(doc);
    }
    save_doc_manifest(
        &crate::models::DocManifest {
            vault_root_raw: manifest.vault_root_raw,
            docs: updated,
        },
        ctx,
    )?;
    Ok(0)
}

#[expect(
    clippy::too_many_lines,
    reason = "mirrors the small Python bootstrap command"
)]
pub fn docs_bootstrap(ctx: &Context, vault_dir: &Path, dry_run: bool, force: bool) -> Result<i32> {
    let vault_dir = absolutize(vault_dir)?;
    if !vault_dir.exists() {
        eprintln!("vault directory not found: {}", vault_dir.display());
        return Ok(2);
    }

    let slices = load_slice_docs(ctx)?;
    let vault_root = relative_path(&vault_dir, ctx.slices_dir());
    let mut entries = BTreeMap::<String, BootstrapEntry>::new();
    let mut unresolved = Vec::<(String, String)>::new();
    for md_file in markdown_files(&vault_dir)? {
        let rel_path = md_file
            .strip_prefix(&vault_dir)
            .unwrap_or(md_file.as_path())
            .to_string_lossy()
            .into_owned();
        let content = std::fs::read_to_string(&md_file).map_err(|source| Error::Read {
            path: md_file.clone(),
            source,
        })?;
        let frontmatter = parse_frontmatter_map(&content)?;
        let doc_id = value_string(frontmatter.get("doc_id"))
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| {
                md_file
                    .file_stem()
                    .map_or_else(String::new, |stem| stem.to_string_lossy().into_owned())
            });
        let mut slice_ids = Vec::new();
        for track in string_list(frontmatter.get("tracks")) {
            if track.to_lowercase().ends_with(".md") {
                continue;
            }
            let resolved = resolve_track_to_slice_ids(&track, &slices, ctx);
            if resolved.is_empty() {
                unresolved.push((doc_id.clone(), track));
            } else {
                for sid in resolved {
                    if !slice_ids.contains(&sid) {
                        slice_ids.push(sid);
                    }
                }
            }
        }
        slice_ids.sort();
        entries.insert(
            doc_id,
            BootstrapEntry {
                path: rel_path,
                slices: slice_ids,
                tags: string_list(frontmatter.get("tags")),
            },
        );
    }

    if entries.is_empty() {
        eprintln!("no .md files found in vault");
        return Ok(1);
    }
    if dry_run {
        print_bootstrap_dry_run(&vault_root, &entries, &unresolved);
        return Ok(0);
    }
    let manifest_path = ctx.docs_manifest_path();
    if manifest_path.exists() && !force {
        eprintln!(
            "{} already exists - use --force to overwrite",
            ctx.rel(&manifest_path)
        );
        return Ok(1);
    }

    let docs = entries
        .iter()
        .map(|(doc_id, entry)| TrackedDoc {
            doc_id: doc_id.clone(),
            path: entry.path.clone(),
            slices: entry.slices.clone(),
            verified_at: String::new(),
            tags: entry.tags.clone(),
            include: Vec::new(),
            exclude: Vec::new(),
            fingerprint: String::new(),
        })
        .collect();
    save_doc_manifest(
        &crate::models::DocManifest {
            vault_root_raw: Some(vault_root),
            docs,
        },
        ctx,
    )?;
    let mapped = entries
        .values()
        .filter(|entry| !entry.slices.is_empty())
        .count();
    println!("wrote {}", ctx.rel(&manifest_path));
    println!("  docs:                {}", entries.len());
    println!("  with slice mappings: {mapped}");
    println!(
        "  without mappings:    {}  (stamp manually or add slices)",
        entries.len() - mapped
    );
    if !unresolved.is_empty() {
        println!("  unresolved tracks:   {}", unresolved.len());
        for (doc_id, track) in unresolved.iter().take(10) {
            println!("    [{doc_id}] {track}");
        }
        if unresolved.len() > 10 {
            println!(
                "    ... and {} more (re-run with --dry-run to see all)",
                unresolved.len() - 10
            );
        }
    }
    Ok(0)
}

pub fn python_fallback(ctx: &Context, command_args: &[String]) -> Result<i32> {
    let mut command = Command::new("python3");
    command
        .args(["-m", "slice_cli", "--repo"])
        .arg(ctx.repo_root())
        .args(["--slices-dir"])
        .arg(ctx.slices_dir())
        .args(command_args);
    if let Some(project_root) = slice_cli_project_root() {
        let python_path = std::env::var_os("PYTHONPATH").map_or_else(
            || project_root.to_string_lossy().into_owned(),
            |existing| {
                format!(
                    "{}:{}",
                    project_root.to_string_lossy(),
                    existing.to_string_lossy()
                )
            },
        );
        command.env("PYTHONPATH", python_path);
    }
    match command.status() {
        Ok(status) => Ok(status.code().unwrap_or(1)),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("python3 is required for this delegated prototype command");
            Ok(2)
        }
        Err(err) => Err(Error::Io(err)),
    }
}

fn slice_cli_project_root() -> Option<&'static Path> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = manifest_dir.parent()?.parent()?;
    root.join("slice_cli").is_dir().then_some(root)
}

fn stamp_targets(
    ctx: &Context,
    docs: &[TrackedDoc],
    doc_id: Option<&str>,
    slice_id: Option<&str>,
    doc_path: Option<&str>,
    stamp_all: bool,
) -> Result<(Vec<String>, i32)> {
    if let Some(doc_id) = doc_id {
        let targets = docs
            .iter()
            .filter(|doc| doc.doc_id == doc_id)
            .map(|doc| doc.doc_id.clone())
            .collect::<Vec<_>>();
        if targets.is_empty() {
            eprintln!("no doc with id '{doc_id}' in manifest");
        }
        return Ok((targets, 1));
    }
    if let Some(slice_id) = slice_id {
        let targets = docs
            .iter()
            .filter(|doc| doc.slices.iter().any(|sid| sid == slice_id))
            .map(|doc| doc.doc_id.clone())
            .collect::<Vec<_>>();
        if targets.is_empty() {
            eprintln!("no docs linked to slice '{slice_id}' in manifest");
        }
        return Ok((targets, 1));
    }
    if let Some(doc_path) = doc_path {
        let targets = docs
            .iter()
            .filter(|doc| doc.path == doc_path)
            .map(|doc| doc.doc_id.clone())
            .collect::<Vec<_>>();
        if targets.is_empty() {
            eprintln!("no doc with path '{doc_path}' in manifest");
        }
        return Ok((targets, 1));
    }
    if stamp_all {
        return Ok((docs.iter().map(|doc| doc.doc_id.clone()).collect(), 0));
    }

    let slices = load_slice_docs(ctx)?;
    let drifted = stale_docs_for(ctx, &slices, docs, StalenessMode::Attributed);
    if drifted.is_empty() {
        println!("all docs are up to date");
    }
    Ok((drifted.into_iter().map(|doc| doc.doc_id).collect(), 0))
}

#[derive(Debug)]
struct BootstrapEntry {
    path: String,
    slices: Vec<String>,
    tags: Vec<String>,
}

fn absolutize(path: &Path) -> Result<std::path::PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

fn markdown_files(root: &Path) -> Result<Vec<std::path::PathBuf>> {
    let mut out = Vec::new();
    collect_markdown_files(root, &mut out)?;
    out.sort();
    Ok(out)
}

fn collect_markdown_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_markdown_files(&path, out)?;
        } else if path.extension().is_some_and(|ext| ext == "md") {
            out.push(path);
        }
    }
    Ok(())
}

fn parse_frontmatter_map(content: &str) -> Result<FxHashMap<String, yaml_serde::Value>> {
    let Some(frontmatter) = content
        .strip_prefix("---\n")
        .and_then(|rest| rest.find("\n---").map(|end| &rest[..end]))
    else {
        return Ok(FxHashMap::default());
    };
    let value = yaml_serde::from_str(frontmatter).map_err(|source| Error::Yaml {
        path: "frontmatter".to_owned(),
        source,
    })?;
    let yaml_serde::Value::Mapping(mapping) = value else {
        return Ok(FxHashMap::default());
    };
    let mut out = FxHashMap::default();
    for (key, value) in mapping {
        if let yaml_serde::Value::String(key) = key {
            out.insert(key, value);
        }
    }
    Ok(out)
}

fn string_list(value: Option<&yaml_serde::Value>) -> Vec<String> {
    match value {
        Some(yaml_serde::Value::Sequence(values)) => values
            .iter()
            .filter_map(|value| value_string(Some(value)))
            .filter(|value| !value.is_empty())
            .collect(),
        Some(value) => value_string(Some(value)).into_iter().collect(),
        None => Vec::new(),
    }
}

fn value_string(value: Option<&yaml_serde::Value>) -> Option<String> {
    match value? {
        yaml_serde::Value::String(value) => Some(value.trim().to_owned()),
        yaml_serde::Value::Number(value) => Some(value.to_string()),
        yaml_serde::Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

fn resolve_track_to_slice_ids(track: &str, slices: &[SliceDoc], ctx: &Context) -> Vec<String> {
    let normalized = crate::paths::normalize_repo_path(track, ctx);
    let dir_prefix = format!("{}/", normalized.trim_end_matches('/'));
    let mut slice_ids = Vec::new();
    for slice in slices {
        let matched = slice.files.iter().any(|file| {
            let file = crate::paths::normalize_repo_path(file, ctx);
            file == normalized || matches_path(&file, &normalized) || file.starts_with(&dir_prefix)
        });
        if matched && !slice_ids.contains(&slice.slice_id) {
            slice_ids.push(slice.slice_id.clone());
        }
    }
    slice_ids.sort();
    slice_ids
}

fn relative_path(path: &Path, base: &Path) -> String {
    pathdiff::diff_paths(path, base)
        .unwrap_or_else(|| path.to_path_buf())
        .to_string_lossy()
        .into_owned()
}

fn print_bootstrap_dry_run(
    vault_root: &str,
    entries: &BTreeMap<String, BootstrapEntry>,
    unresolved: &[(String, String)],
) {
    println!("vault_root: {vault_root}");
    println!("docs found: {}", entries.len());
    let mapped = entries
        .values()
        .filter(|entry| !entry.slices.is_empty())
        .count();
    println!("  with slice mappings: {mapped}");
    println!("  without mappings:    {}", entries.len() - mapped);
    println!();
    for (doc_id, entry) in entries {
        let slices = if entry.slices.is_empty() {
            "(no slices)".to_owned()
        } else {
            entry.slices.join(", ")
        };
        println!("  {doc_id}");
        println!("    path:   {}", entry.path);
        println!("    slices: {slices}");
    }
    if !unresolved.is_empty() {
        println!("\nunresolved tracks ({}):", unresolved.len());
        for (doc_id, track) in unresolved {
            println!("  [{doc_id}] {track}");
        }
    }
}

#[must_use]
pub fn stale_docs_for(
    ctx: &Context,
    slices: &[SliceDoc],
    tracked_docs: &[crate::models::TrackedDoc],
    mode: StalenessMode,
) -> Vec<StaleDoc> {
    let by_id = slices
        .iter()
        .map(|slice| (slice.slice_id.as_str(), slice))
        .collect::<FxHashMap<_, _>>();
    let mut stale = Vec::new();
    for doc in tracked_docs {
        let linked_slices = doc
            .slices
            .iter()
            .filter(|sid| by_id.contains_key(sid.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        let tracked_files = resolve_tracked_files(doc, &by_id);
        if tracked_files.is_empty() {
            continue;
        }
        let concrete = tracked_files
            .iter()
            .flat_map(|file| expand_literal_or_existing(file, ctx))
            .map(|file| ctx.git_relative_path(&file))
            .collect::<Vec<_>>();
        if doc.fingerprint.is_empty() {
            if doc.verified_at.is_empty() {
                stale.push(StaleDoc {
                    affected_slices: linked_slices,
                    changed_files: tracked_files,
                    doc_id: doc.doc_id.clone(),
                    path: doc.path.clone(),
                    verified_at: "(never)".to_owned(),
                });
            } else if matches!(mode, StalenessMode::Fast) {
                stale.push(StaleDoc {
                    affected_slices: linked_slices,
                    changed_files: concrete,
                    doc_id: doc.doc_id.clone(),
                    path: doc.path.clone(),
                    verified_at: doc.verified_at.clone(),
                });
            } else {
                match git_changed_files(ctx, &tracked_files, &doc.verified_at) {
                    GitChanges::Changed(changed) if !changed.is_empty() => {
                        stale.push(StaleDoc {
                            affected_slices: affected_slices(&changed, &linked_slices, &by_id)
                                .unwrap_or(linked_slices),
                            changed_files: changed,
                            doc_id: doc.doc_id.clone(),
                            path: doc.path.clone(),
                            verified_at: doc.verified_at.clone(),
                        });
                    }
                    GitChanges::BadRevision => stale.push(StaleDoc {
                        affected_slices: linked_slices,
                        changed_files: vec![format!(
                            "(git error: unable to resolve {})",
                            doc.verified_at
                        )],
                        doc_id: doc.doc_id.clone(),
                        path: doc.path.clone(),
                        verified_at: doc.verified_at.clone(),
                    }),
                    GitChanges::Changed(_) => {}
                }
            }
            continue;
        }
        if content_fingerprint(ctx, &concrete) != doc.fingerprint {
            let changed = if matches!(mode, StalenessMode::Fast) {
                concrete
            } else {
                match git_changed_files(ctx, &tracked_files, &doc.verified_at) {
                    GitChanges::Changed(changed) if !changed.is_empty() => changed,
                    GitChanges::Changed(_) | GitChanges::BadRevision => concrete,
                }
            };
            stale.push(StaleDoc {
                affected_slices: affected_slices(&changed, &linked_slices, &by_id)
                    .unwrap_or(linked_slices),
                changed_files: changed,
                doc_id: doc.doc_id.clone(),
                path: doc.path.clone(),
                verified_at: doc.verified_at.clone(),
            });
        }
    }
    stale
}

#[derive(Debug, Clone, Copy)]
pub enum StalenessMode {
    Fast,
    Attributed,
}

enum GitChanges {
    Changed(Vec<String>),
    BadRevision,
}

fn git_changed_files(ctx: &Context, files: &[String], verified_at: &str) -> GitChanges {
    let mut changed = FxHashSet::default();
    if !verified_at.is_empty() {
        let mut command = Command::new("git");
        command
            .args(["diff", "--name-only", &format!("{verified_at}..HEAD"), "--"])
            .args(files)
            .current_dir(ctx.repo_root());
        match command.output() {
            Ok(output) if output.status.success() => {
                changed.extend(
                    String::from_utf8_lossy(&output.stdout)
                        .lines()
                        .map(str::trim)
                        .filter(|line| !line.is_empty())
                        .map(ToOwned::to_owned),
                );
            }
            Ok(_) => return GitChanges::BadRevision,
            Err(_) => return GitChanges::Changed(Vec::new()),
        }
    }

    let mut command = Command::new("git");
    command
        .args(["diff", "--name-only", "HEAD", "--"])
        .args(files)
        .current_dir(ctx.repo_root());
    if let Ok(output) = command.output()
        && output.status.success()
    {
        changed.extend(
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(ToOwned::to_owned),
        );
    }

    let mut ordered = changed.into_iter().collect::<Vec<_>>();
    ordered.sort();
    GitChanges::Changed(ordered)
}

fn affected_slices(
    changed: &[String],
    linked_slices: &[String],
    by_id: &FxHashMap<&str, &SliceDoc>,
) -> Option<Vec<String>> {
    let affected = linked_slices
        .iter()
        .filter(|sid| {
            by_id.get(sid.as_str()).is_some_and(|slice| {
                changed.iter().any(|changed_file| {
                    slice
                        .files
                        .iter()
                        .any(|file| matches_path(changed_file, file))
                })
            })
        })
        .cloned()
        .collect::<Vec<_>>();
    (!affected.is_empty()).then_some(affected)
}

fn require_slice<'a>(docs: &'a [SliceDoc], selector: &str) -> Result<&'a SliceDoc> {
    slice_for_selector(docs, selector).ok_or_else(|| Error::UnknownSlice(selector.to_owned()))
}

fn show_sections(doc: &SliceDoc, mode: ShowMode, json_output: bool) -> Result<i32> {
    if matches!(mode, ShowMode::Body) {
        if json_output {
            emit_json(&json!({"body": doc.body, "slice_id": doc.slice_id}))?;
        } else {
            println!("{}", doc.body);
        }
        return Ok(0);
    }

    let sections = extract_sections(&doc.body);
    let names = requested_section_names(mode);
    if json_output {
        let present = names
            .iter()
            .filter_map(|name| {
                section_text(&sections, name).map(|text| ((*name).to_owned(), text.to_owned()))
            })
            .collect::<std::collections::BTreeMap<_, _>>();
        emit_json(&json!({"sections": present, "slice_id": doc.slice_id}))?;
        return Ok(0);
    }

    for name in names {
        println!("{name}:");
        println!(
            "{}",
            section_text(&sections, name).unwrap_or("  (not present)")
        );
        println!();
    }
    Ok(0)
}

fn requested_section_names(mode: ShowMode) -> Vec<&'static str> {
    match mode {
        ShowMode::Metadata | ShowMode::Body => Vec::new(),
        ShowMode::System => STANDARD_SECTIONS.to_vec(),
        ShowMode::CallStacks => vec!["Runtime Flows"],
        ShowMode::Verification => vec!["Verification", "Update Triggers"],
    }
}

fn find_matches<'a>(
    docs: &'a [SliceDoc],
    tracked_docs: &[crate::models::TrackedDoc],
    needle: &str,
) -> Vec<FindMatch<'a>> {
    let text = needle.to_lowercase();
    let mut tags_by_slice = FxHashMap::<&str, Vec<&str>>::default();
    for tracked in tracked_docs {
        for sid in &tracked.slices {
            let tags = tags_by_slice.entry(sid.as_str()).or_default();
            tags.extend(tracked.tags.iter().map(String::as_str));
        }
    }

    let mut rows = Vec::new();
    for doc in docs {
        let mut fields = Vec::new();
        if doc.slice_id.to_lowercase().contains(&text) {
            fields.push("slice_id");
        }
        if doc.description.to_lowercase().contains(&text) {
            fields.push("description");
        }
        if doc
            .files
            .iter()
            .any(|file| file.to_lowercase().contains(&text))
        {
            fields.push("files");
        }
        if doc
            .abstractions
            .iter()
            .any(|abstraction| abstraction.to_lowercase().contains(&text))
        {
            fields.push("abstractions");
        }
        if doc
            .dependencies
            .iter()
            .any(|dependency| dependency.to_lowercase().contains(&text))
        {
            fields.push("dependencies");
        }
        if tags_by_slice
            .get(doc.slice_id.as_str())
            .is_some_and(|tags| tags.iter().any(|tag| tag.to_lowercase().contains(&text)))
        {
            fields.push("doc_tags");
        }
        if doc.body.to_lowercase().contains(&text) {
            fields.push("body");
        }
        if !fields.is_empty() {
            rows.push(FindMatch {
                description: &doc.description,
                matches: fields,
                slice_id: &doc.slice_id,
            });
        }
    }
    rows
}

fn reverse_deps(docs: &[SliceDoc]) -> FxHashMap<String, Vec<String>> {
    let mut reverse = docs
        .iter()
        .map(|doc| (doc.slice_id.clone(), Vec::new()))
        .collect::<FxHashMap<_, _>>();
    for doc in docs {
        for dep in &doc.dependencies {
            reverse
                .entry(dep.clone())
                .or_default()
                .push(doc.slice_id.clone());
        }
    }
    for values in reverse.values_mut() {
        values.sort();
    }
    reverse
}

fn transitive_deps(start: &str, docs: &[SliceDoc]) -> Vec<String> {
    let deps = docs
        .iter()
        .map(|doc| (doc.slice_id.as_str(), doc.dependencies.as_slice()))
        .collect::<FxHashMap<_, _>>();
    let mut ordered = Vec::new();
    let mut seen = FxHashSet::default();
    let mut queue = VecDeque::from(deps.get(start).copied().unwrap_or_default().to_vec());
    while let Some(current) = queue.pop_front() {
        if !seen.insert(current.clone()) {
            continue;
        }
        ordered.push(current.clone());
        if let Some(next) = deps.get(current.as_str()) {
            for dep in *next {
                if !seen.contains(dep) {
                    queue.push_back(dep.clone());
                }
            }
        }
    }
    ordered
}

fn resolve_tracked_files(
    doc: &crate::models::TrackedDoc,
    by_id: &FxHashMap<&str, &SliceDoc>,
) -> Vec<String> {
    let mut files = if doc.include.is_empty() {
        let mut files = Vec::new();
        for sid in &doc.slices {
            if let Some(slice) = by_id.get(sid.as_str()) {
                files.extend(slice.files.clone());
            }
        }
        files
    } else {
        doc.include.clone()
    };
    if !doc.exclude.is_empty() {
        files.retain(|file| {
            !doc.exclude
                .iter()
                .any(|exclude| matches_path(file, exclude))
        });
    }
    files
}

pub(crate) fn content_fingerprint(ctx: &Context, rel_paths: &[String]) -> String {
    let mut unique = rel_paths.to_vec();
    unique.sort();
    unique.dedup();
    let mut digest = Sha256::new();
    digest.update(b"slice-content-v1\0");
    for rel in unique {
        digest.update(rel.as_bytes());
        digest.update(b"\0");
        let path = repo_join(ctx, &rel);
        if path.is_file() {
            match std::fs::read(path) {
                Ok(bytes) => digest.update(bytes),
                Err(_) => digest.update(b"<deleted>"),
            }
        } else {
            digest.update(b"<deleted>");
        }
        digest.update(b"\0");
    }
    format!("{:x}", digest.finalize())
}

fn present_sections(body: &str) -> std::collections::BTreeMap<String, String> {
    let parsed = extract_sections(body);
    STANDARD_SECTIONS
        .into_iter()
        .filter_map(|standard| {
            section_text(&parsed, standard).map(|text| (standard.to_owned(), text.to_owned()))
        })
        .collect()
}

fn extract_sections(body: &str) -> Vec<(String, String)> {
    let mut sections = Vec::new();
    let mut current: Option<String> = None;
    let mut buffer = Vec::new();

    for line in body.lines() {
        if let Some(heading) = line.strip_prefix("## ") {
            if let Some(name) = current.replace(heading.trim().to_owned()) {
                sections.push((name, buffer.join("\n").trim_matches('\n').to_owned()));
                buffer.clear();
            }
        } else if current.is_some() {
            buffer.push(line);
        }
    }
    if let Some(name) = current {
        sections.push((name, buffer.join("\n").trim_matches('\n').to_owned()));
    }
    sections
}

fn section_text<'a>(sections: &'a [(String, String)], name: &str) -> Option<&'a str> {
    sections
        .iter()
        .find(|(heading, _)| heading.eq_ignore_ascii_case(name))
        .map(|(_, text)| text.as_str())
}

pub(crate) fn emit_json<T: Serialize>(value: &T) -> Result<()> {
    let stdout = io::stdout();
    let mut lock = stdout.lock();
    serde_json::to_writer_pretty(&mut lock, value)?;
    writeln!(&mut lock)?;
    Ok(())
}

fn print_list(label: &str, values: &[String]) {
    if values.is_empty() {
        println!("{label}: (none)");
    } else {
        println!("{label}:");
        for value in values {
            println!("  - {value}");
        }
    }
}
