use std::collections::VecDeque;
use std::io::{self, Write};

use rustc_hash::{FxHashMap, FxHashSet};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::context::Context;
use crate::manifest::load_doc_manifest;
use crate::models::{
    AffectedDoc, ContextDoc, ContextOutput, ContextSlice, DepsOutput, ListRow, ShowSlice, SliceDoc,
    SliceOwner, StaleDoc, TrackedDocSummary,
};
use crate::paths::{expand_literal_or_existing, matches_path, repo_join};
use crate::slices::{docs_for_slice, load_slice_docs, owners_for_path, slice_for_selector};
use crate::{Error, Result};

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

pub fn show(ctx: &Context, selector: &str, json: bool) -> Result<i32> {
    let docs = load_slice_docs(ctx)?;
    let doc = require_slice(&docs, selector)?;
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

    let mut affected_docs = manifest
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

    affected_docs.sort_by(|a, b| a.doc_id.cmp(&b.doc_id));
    let stale = stale_docs_for(ctx, &docs, &manifest.docs);
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

pub fn context(ctx: &Context, selector: &str, json: bool) -> Result<i32> {
    let docs = load_slice_docs(ctx)?;
    let manifest = load_doc_manifest(ctx)?;
    let mut targets = owners_for_path(&docs, selector, ctx);
    if targets.is_empty() {
        targets.push(require_slice(&docs, selector)?);
    }
    targets.sort_by(|a, b| a.slice_id.cmp(&b.slice_id));

    let stale_ids = stale_docs_for(ctx, &docs, &manifest.docs)
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

pub fn stale_docs(ctx: &Context, json: bool) -> Result<i32> {
    let docs = load_slice_docs(ctx)?;
    let manifest = load_doc_manifest(ctx)?;
    let stale = stale_docs_for(ctx, &docs, &manifest.docs);
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

#[must_use]
pub fn stale_docs_for(
    ctx: &Context,
    slices: &[SliceDoc],
    tracked_docs: &[crate::models::TrackedDoc],
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
            stale.push(StaleDoc {
                affected_slices: linked_slices,
                changed_files: concrete,
                doc_id: doc.doc_id.clone(),
                path: doc.path.clone(),
                verified_at: if doc.verified_at.is_empty() {
                    "(never)".to_owned()
                } else {
                    doc.verified_at.clone()
                },
            });
            continue;
        }
        if content_fingerprint(ctx, &concrete) != doc.fingerprint {
            stale.push(StaleDoc {
                affected_slices: linked_slices,
                changed_files: concrete,
                doc_id: doc.doc_id.clone(),
                path: doc.path.clone(),
                verified_at: doc.verified_at.clone(),
            });
        }
    }
    stale.sort_by(|a, b| a.doc_id.cmp(&b.doc_id));
    stale
}

fn require_slice<'a>(docs: &'a [SliceDoc], selector: &str) -> Result<&'a SliceDoc> {
    slice_for_selector(docs, selector).ok_or_else(|| Error::UnknownSlice(selector.to_owned()))
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

fn content_fingerprint(ctx: &Context, rel_paths: &[String]) -> String {
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
    const STANDARD: [&str; 5] = [
        "System Behavior",
        "Invariants",
        "Runtime Flows",
        "Verification",
        "Update Triggers",
    ];
    let parsed = extract_sections(body);
    STANDARD
        .into_iter()
        .filter_map(|standard| {
            parsed
                .iter()
                .find(|(heading, _)| heading.eq_ignore_ascii_case(standard))
                .map(|(_, text)| (standard.to_owned(), text.clone()))
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

fn emit_json<T: Serialize>(value: &T) -> Result<()> {
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
