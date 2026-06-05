use std::collections::{BTreeMap, VecDeque};
use std::io::{self, Write};
use std::path::Path;
use std::process::{Command, Stdio};

use rustc_hash::{FxHashMap, FxHashSet};
use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::color::{ColorChoice, Styles, shell_quote};
use crate::config::{Ambiguity, load_ambiguity};
use crate::context::Context;
use crate::git_backend::{GitBackend, GitChanges, ProcessGitBackend};
use crate::manifest::{load_doc_manifest, save_doc_manifest};
use crate::models::{
    AffectedDoc, CollaboratorFile, ContextDoc, ContextOutput, ContextSlice, DepsOutput, FindMatch,
    ListRow, ShowSlice, SliceDoc, SliceDocStatus, SliceOwner, StaleDoc, TrackedDoc,
    TrackedDocSummary,
};
use crate::paths::{expand_literal_or_existing, matches_path, repo_join};
use crate::slices::{
    docs_for_slice, load_slice_body, load_slice_docs, load_slice_docs_meta, owners_for_path,
    slice_for_selector,
};
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

pub fn list(ctx: &Context, json: bool, styles: &Styles) -> Result<i32> {
    let docs = load_slice_docs_meta(ctx)?;
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
        // Stale set is a human-path-only concern; the --json shape above is unchanged.
        let stale_ids = stale_docs_for(ctx, &docs, &manifest.docs, StalenessMode::Fast)
            .into_iter()
            .map(|stale| stale.doc_id)
            .collect::<FxHashSet<_>>();
        let width = docs
            .iter()
            .map(|doc| doc.slice_id.len())
            .max()
            .unwrap_or(10);
        for doc in &docs {
            let pad = " ".repeat(width.saturating_sub(doc.slice_id.len()));
            let loc = doc.loc.map_or(String::new(), |loc| format!(" ({loc} LoC)"));
            let linked = docs_for_slice(&manifest.docs, &doc.slice_id);
            let doc_label = if linked.is_empty() {
                String::new()
            } else {
                format!(" [{} docs]", linked.len())
            };
            let stale_count = linked
                .iter()
                .filter(|tracked| stale_ids.contains(&tracked.doc_id))
                .count();
            let stale_label = if stale_count == 0 {
                String::new()
            } else {
                styles.paint(styles.stale, &format!(" [{stale_count} stale]"))
            };
            println!(
                "{}{pad}  {}{}{}{}",
                styles.paint(styles.id, &doc.slice_id),
                doc.description,
                styles.paint(styles.dim, &loc),
                styles.paint(styles.dim, &doc_label),
                stale_label,
            );
        }
    }
    Ok(0)
}

pub fn show(
    ctx: &Context,
    selector: &str,
    mode: ShowMode,
    json: bool,
    styles: &Styles,
) -> Result<i32> {
    let docs = if matches!(mode, ShowMode::Metadata) {
        load_slice_docs_meta(ctx)?
    } else {
        load_slice_docs(ctx)?
    };
    let doc = require_slice(&docs, selector)?;
    if !matches!(mode, ShowMode::Metadata) {
        return show_sections(doc, mode, json);
    }
    let manifest = load_doc_manifest(ctx)?;
    let linked_docs = docs_for_slice(&manifest.docs, &doc.slice_id);
    // The lede serves both the json `overview` field and the human overview block.
    let lede = slice_lede(&load_slice_body(ctx, doc)?);
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
            overview: lede,
            slice_id: &doc.slice_id,
        };
        emit_json(&output)?;
    } else {
        // verified_at is reddened when stale; compute the stale set over linked docs.
        let relevant = linked_docs.iter().copied().cloned().collect::<Vec<_>>();
        let stale_ids = stale_docs_for(ctx, &docs, &relevant, StalenessMode::Fast)
            .into_iter()
            .map(|stale| stale.doc_id)
            .collect::<FxHashSet<_>>();
        let key = |label: &str| styles.paint(styles.dim, label);
        println!(
            "{} {}",
            key("slice_id:"),
            styles.paint(styles.id, &doc.slice_id)
        );
        println!("{} {}", key("description:"), doc.description);
        if !lede.is_empty() {
            println!("{}", key("overview:"));
            for line in lede.lines() {
                if line.is_empty() {
                    println!();
                } else {
                    println!("  {line}");
                }
            }
            println!();
        }
        println!(
            "{} {}",
            key("loc:"),
            doc.loc.map_or_else(|| "null".to_owned(), |v| v.to_string())
        );
        println!("{} {}", key("doc_path:"), ctx.rel(&doc.doc_path));
        print_list_colored("files", &doc.files, styles, None);
        print_list_colored("dependencies", &doc.dependencies, styles, Some(styles.dep));
        print_abstractions(ctx, &doc.files, &doc.abstractions, styles);
        print_list_colored("exclusions", &doc.exclusions, styles, None);
        print_tracked_docs("docs", &linked_docs, styles, &stale_ids);
    }
    Ok(0)
}

pub fn files(ctx: &Context, selector: &str, json: bool) -> Result<i32> {
    let docs = load_slice_docs_meta(ctx)?;
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

// The four bools mirror `deps`' clap flags one-to-one; the only caller is the clap dispatch in
// `cli.rs`, which destructures them by name, so there's no positional-boolean ambiguity to guard.
#[allow(clippy::fn_params_excessive_bools)]
pub fn deps(
    ctx: &Context,
    selector: &str,
    reverse: bool,
    transitive: bool,
    files: bool,
    json: bool,
) -> Result<i32> {
    let docs = load_slice_docs_meta(ctx)?;
    let doc = require_slice(&docs, selector)?;
    let (dependencies, mode) = if reverse && transitive {
        (
            transitive_reverse_deps(&doc.slice_id, &docs),
            "reverse-transitive",
        )
    } else if reverse {
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

    // `--files` resolves the dependency slices to their concrete files — the blast-radius
    // "candidate file discovery" hop. Opt-in, so the default output is unchanged.
    let collaborator_files = files.then(|| collaborator_files(&dependencies, &docs));

    if json {
        emit_json(&DepsOutput {
            dependencies,
            mode,
            slice_id: &doc.slice_id,
            files: collaborator_files,
        })?;
    } else if let Some(collaborators) = collaborator_files {
        // File-level provenance: `file<TAB>slice_id`, the file (the actionable read target) first.
        for cf in collaborators {
            println!("{}\t{}", cf.file, cf.slice_id);
        }
    } else {
        for dep in dependencies {
            println!("{dep}");
        }
    }
    Ok(0)
}

/// Resolve dependency slice ids to their concrete files for `deps --files`. File-level by design —
/// `file:line` precision lives at the read (`grep --symbols`), per the layered-provenance invariant.
///
/// Scoping rule (the unit test is the spec):
/// - output order follows `dependency_ids`, which for `--reverse --transitive` is BFS distance order
///   (nearest collaborators first) — so ranking is by reverse-dep distance;
/// - within a slice, files keep their declared order;
/// - a file owned by several collaborators is emitted **once**, attributed to the first (nearest)
///   slice that owns it.
///
/// No cap: dropping a collaborator file could hide gold (the kill-condition concern). Dedupe +
/// distance ranking is the scoping; over-pull is handled by the gated anchoring escalation, never by
/// silently truncating here.
fn collaborator_files(dependency_ids: &[String], docs: &[SliceDoc]) -> Vec<CollaboratorFile> {
    let by_id: FxHashMap<&str, &SliceDoc> = docs.iter().map(|d| (d.slice_id.as_str(), d)).collect();
    let mut seen = FxHashSet::default();
    let mut out = Vec::new();
    for sid in dependency_ids {
        let Some(doc) = by_id.get(sid.as_str()) else {
            continue; // dependency id with no matching slice doc — nothing to list
        };
        for file in &doc.files {
            if seen.insert(file.clone()) {
                out.push(CollaboratorFile {
                    slice_id: doc.slice_id.clone(),
                    file: file.clone(),
                });
            }
        }
    }
    out
}

/// Slices that reference `path` as a Verification target (e.g. a test file), used as a fallback for
/// `for`/`context` when no `files:` owner matches. Returns `(slice_id, description)`. Loads full-body
/// docs because the Verification links live in the body (the metadata loader drops it); this is a
/// cold path, so the extra read is fine. Does NOT touch `owners_for_path`, so `affected-docs` and
/// staleness are unaffected. Verification refs only — `exclusions:` is a disown signal, not ownership.
fn verification_associations(ctx: &Context, path: &str) -> Result<Vec<(String, String)>> {
    let normalized = crate::paths::normalize_repo_path(path, ctx);
    let docs = load_slice_docs(ctx)?;
    let mut out = Vec::new();
    for doc in &docs {
        let referenced = crate::check::parse_verification(&doc.body)
            .into_iter()
            .flat_map(|(_, refs)| refs)
            .any(|reference| {
                // refs look like `tests/foo.py::TestBar`; match on the file part only, normalized
                // the same way the query path is (e.g. strip a leading `./`) so the forms line up.
                let file = reference.split("::").next().unwrap_or(&reference).trim();
                let file = crate::paths::normalize_repo_path(file, ctx);
                matches_path(&normalized, &file)
            });
        if referenced {
            out.push((doc.slice_id.clone(), doc.description.clone()));
        }
    }
    Ok(out)
}

pub fn for_path(ctx: &Context, path: &str, json: bool) -> Result<i32> {
    let docs = load_slice_docs_meta(ctx)?;
    let owners = owners_for_path(&docs, path, ctx);
    if json {
        // JSON contract unchanged: `files:` owners only (verification fallback is human-only).
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
        let normalized = crate::paths::normalize_repo_path(path, ctx);
        let associations = verification_associations(ctx, path)?;
        if associations.is_empty() {
            eprintln!(
                "no slice association for: {normalized} (checked files and verification links)"
            );
            return Ok(1);
        }
        // Third tab-field keeps the slice_id<TAB>description contract clean.
        for (slice_id, description) in associations {
            println!("{slice_id}\t{description}\t(via verification: {normalized})");
        }
        return Ok(0);
    }
    for owner in owners {
        println!("{}\t{}", owner.slice_id, owner.description);
    }
    Ok(0)
}

pub fn affected_docs(ctx: &Context, paths: &[String], json: bool) -> Result<i32> {
    let docs = load_slice_docs_meta(ctx)?;
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

#[expect(
    clippy::too_many_lines,
    reason = "single resolve→render path; splitting the borrow-coupled fragments hurts readability"
)]
pub fn context(
    ctx: &Context,
    selector: &str,
    strict: bool,
    best_effort: bool,
    json: bool,
) -> Result<i32> {
    let docs = load_slice_docs_meta(ctx)?;
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
    let mut via_verification = false;
    if targets.is_empty() {
        if let Some(doc) = slice_for_selector(&docs, selector) {
            targets = vec![doc];
        } else if json {
            // JSON contract unchanged: no verification fallback on the machine path.
            eprintln!(
                "no owning slice for: {}",
                crate::paths::normalize_repo_path(selector, ctx)
            );
            return Ok(1);
        } else {
            // Human-only fallback: resolve a test file to the slice(s) that reference it.
            let ids: FxHashSet<String> = verification_associations(ctx, selector)?
                .into_iter()
                .map(|(slice_id, _)| slice_id)
                .collect();
            targets = docs.iter().filter(|d| ids.contains(&d.slice_id)).collect();
            if targets.is_empty() {
                eprintln!(
                    "no slice association for: {} (checked files and verification links)",
                    crate::paths::normalize_repo_path(selector, ctx)
                );
                return Ok(1);
            }
            via_verification = true;
        }
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
                let body = load_slice_body(ctx, doc)?;
                let linked_docs = docs_for_slice(&manifest.docs, &doc.slice_id)
                    .into_iter()
                    .map(|tracked| ContextDoc {
                        doc_id: tracked.doc_id.clone(),
                        path: tracked.path.clone(),
                        stale: stale_ids.contains(&tracked.doc_id),
                        verified_at: tracked.verified_at.clone(),
                    })
                    .collect();
                Ok(ContextSlice {
                    dependencies: doc.dependencies.clone(),
                    description: doc.description.clone(),
                    doc_path: ctx.rel(&doc.doc_path),
                    docs: linked_docs,
                    files: doc.files.clone(),
                    sections: present_sections(&body),
                    slice_id: doc.slice_id.clone(),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        emit_json(&ContextOutput { slices })?;
    } else {
        for doc in targets {
            println!("slice: {}", doc.slice_id);
            println!("description: {}", doc.description);
            if via_verification {
                println!(
                    "association: via verification ({})",
                    crate::paths::normalize_repo_path(selector, ctx)
                );
            }
            println!("doc: {}", ctx.rel(&doc.doc_path));
            println!("files: {}", doc.files.join(", "));
            println!("dependencies: {}", doc.dependencies.join(", "));
            // Affordance (principle P): advertise the blast radius factually so the agent pulls it in
            // instead of falling back to brute-force grep (FINDINGS failure mode #2). Human output
            // only — the JSON contract (ContextOutput) stays byte-identical.
            let blast = transitive_reverse_deps(&doc.slice_id, &docs);
            if !blast.is_empty() {
                let n_files = collaborator_files(&blast, &docs).len();
                let n_slices = blast.len();
                let plural = |n: usize| if n == 1 { "" } else { "s" };
                println!(
                    "blast-radius: {n_files} file{} in {n_slices} reverse-dep slice{} — slice deps {} --reverse --transitive --files",
                    plural(n_files),
                    plural(n_slices),
                    doc.slice_id
                );
            }
            let body = load_slice_body(ctx, doc)?;
            for (heading, text) in present_sections(&body) {
                println!("{heading}:");
                println!("{text}");
            }
        }
    }
    Ok(0)
}

pub fn find(ctx: &Context, needle: &str, json: bool, styles: &Styles) -> Result<i32> {
    let docs = load_slice_docs(ctx)?;
    let manifest = load_doc_manifest(ctx)?;
    let matches = find_matches(&docs, &manifest.docs, needle);
    if json {
        emit_json(&matches)?;
        return Ok(i32::from(matches.is_empty()));
    }
    if matches.is_empty() {
        // For a multi-term query, surface which term(s) hit nothing anywhere so the caller knows
        // what to drop, instead of a blind "no matches". A term is globally absent iff finding it
        // alone yields nothing.
        let terms: Vec<&str> = needle.split_whitespace().collect();
        if terms.len() > 1 {
            let unmatched: Vec<&str> = terms
                .iter()
                .copied()
                .filter(|term| find_matches(&docs, &manifest.docs, term).is_empty())
                .collect();
            if !unmatched.is_empty() {
                eprintln!(
                    "no matches for: {needle} (unmatched: {})",
                    unmatched.join(", ")
                );
                return Ok(1);
            }
        }
        eprintln!("no matches for: {needle}");
        return Ok(1);
    }
    let width = matches
        .iter()
        .map(|row| row.slice_id.len())
        .max()
        .unwrap_or(0);
    for row in matches {
        // slice_id keeps its id color; the needle is highlighted only in the
        // description (no nested styles), so strip_ansi() round-trips cleanly.
        let pad = " ".repeat(width.saturating_sub(row.slice_id.len()));
        let labels = styles.paint(styles.label, &format!("[{}]", row.matches.join(",")));
        println!(
            "{}{pad}  {}  {}",
            styles.paint(styles.id, row.slice_id),
            labels,
            styles.highlight(row.description, needle),
        );
    }
    Ok(0)
}

/// Build the fzf rows: `slice_id  description (LoC)`, the id left-padded to a column.
/// The id is shown (and kept first) so it's both visible and fuzzy-searchable — fzf
/// matches the whole line, so typing a slice name filters by id. The id stays free of
/// ANSI so the `{1}` placeholder resolves to a clean value. Slices whose id contains
/// whitespace are skipped (returned in the second element) so the first-token contract
/// can't be corrupted.
fn build_browse_rows(docs: &[SliceDoc], styles: &Styles) -> (String, Vec<String>) {
    let mut skipped = Vec::new();
    let valid = docs
        .iter()
        .filter(|doc| {
            let bad = doc.slice_id.is_empty() || doc.slice_id.chars().any(char::is_whitespace);
            if bad {
                skipped.push(doc.slice_id.clone());
            }
            !bad
        })
        .collect::<Vec<_>>();
    let width = valid
        .iter()
        .map(|doc| doc.slice_id.len())
        .max()
        .unwrap_or(0);
    let mut rows = String::new();
    for doc in valid {
        let pad = " ".repeat(width.saturating_sub(doc.slice_id.len()));
        let loc = doc.loc.map_or(String::new(), |loc| format!(" ({loc} LoC)"));
        rows.push_str(&doc.slice_id);
        rows.push_str(&pad);
        rows.push_str("  ");
        rows.push_str(&doc.description);
        rows.push_str(&styles.paint(styles.dim, &loc));
        rows.push('\n');
    }
    (rows, skipped)
}

/// Extract the `slice_id` (first whitespace-delimited token) from a selected fzf line.
fn selected_slice_id(line: &str) -> &str {
    line.split_whitespace().next().unwrap_or("")
}

/// Wrap `cmd` in an fzf action delimiter that does not occur in `cmd`, so a path
/// containing `)` can't prematurely close a `change-preview(...)` action.
///
/// Refuses (rather than emitting a known-broken `(...)` action) when `cmd` contains
/// every candidate delimiter — only reachable with a pathological repo path holding all
/// of `()[]<>~!@#%^|`. fzf's bind grammar has no escape for the active delimiter, so a
/// broken bind is the only alternative.
fn fzf_action(action: &str, cmd: &str) -> Result<String> {
    for (open, close) in [('(', ')'), ('[', ']'), ('<', '>')] {
        if !cmd.contains(open) && !cmd.contains(close) {
            return Ok(format!("{action}{open}{cmd}{close}"));
        }
    }
    for delim in ['~', '!', '@', '#', '%', '^', '|'] {
        if !cmd.contains(delim) {
            return Ok(format!("{action}{delim}{cmd}{delim}"));
        }
    }
    Err(Error::InvalidInput(format!(
        "repo path contains every fzf action delimiter; cannot build a `{action}` bind"
    )))
}

pub fn browse(
    ctx: &Context,
    query: Option<&str>,
    print: bool,
    styles: &Styles,
    color: ColorChoice,
) -> Result<i32> {
    let docs = load_slice_docs_meta(ctx)?;
    if docs.is_empty() {
        eprintln!("no slices found in slices/");
        return Ok(1);
    }

    // The fzf list and preview always carry color (it renders via --ansi), except
    // when the user explicitly opted out with --color=never.
    let view_choice = if matches!(color, ColorChoice::Never) {
        ColorChoice::Never
    } else {
        ColorChoice::Always
    };
    let view_styles = Styles::resolve(view_choice);
    let preview_color = if matches!(color, ColorChoice::Never) {
        "never"
    } else {
        "always"
    };

    let (rows, skipped) = build_browse_rows(&docs, &view_styles);
    for id in &skipped {
        eprintln!("skipping slice with malformed id: {id:?}");
    }

    // fzf runs preview/bind commands via $SHELL -c, so the exe and repo paths must
    // be shell-quoted; `{1}` is left for fzf to substitute and quote itself.
    let exe = std::env::current_exe()?;
    let q_exe = shell_quote(&exe.to_string_lossy());
    let q_repo = shell_quote(&ctx.repo_root().to_string_lossy());
    let base = format!("{q_exe} --repo {q_repo} --color={preview_color}");
    // `--` precedes the `{1}` slice id so an id starting with `-` is read as a positional
    // selector, not a clap flag; trailing flags stay before the `--`.
    let preview = format!("{base} show -- {{1}}");

    // Lenses onto a slice's three content layers. The keys avoid fzf's default
    // line-editing binds (ctrl-u=clear-query etc.); files/direct-deps are already in
    // the overview, so they don't get their own keys.
    let mut command = Command::new("fzf");
    command
        .arg("--ansi")
        .arg("--preview-window=right,wrap")
        .arg("--preview")
        .arg(&preview)
        .arg("--bind")
        .arg(format!(
            "ctrl-o:{}",
            fzf_action("change-preview", &format!("{base} show -- {{1}}"))?
        ))
        .arg("--bind")
        .arg(format!(
            "ctrl-r:{}",
            fzf_action("change-preview", &format!("{base} show --call-stacks -- {{1}}"))?
        ))
        .arg("--bind")
        .arg(format!(
            "ctrl-d:{}",
            fzf_action("change-preview", &format!("{base} show --verification -- {{1}}"))?
        ))
        .arg("--bind")
        .arg(format!(
            "ctrl-t:{}",
            fzf_action("change-preview", &format!("{base} deps --reverse -- {{1}}"))?
        ))
        .arg("--header")
        .arg("enter: show | ^o overview | ^r runtime | ^d verify | ^t used-by | ^/ pane | esc: cancel")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped());
    if let Some(query) = query {
        command.arg("-q").arg(query);
    }

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("fzf >= 0.30 is required for `slice browse`; install fzf and retry");
            return Ok(2);
        }
        Err(err) => return Err(Error::Io(err)),
    };

    // Feed candidates on a thread; swallow BrokenPipe if fzf is dismissed early.
    let mut stdin = child.stdin.take().expect("piped stdin");
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(rows.as_bytes());
    });
    let output = child.wait_with_output()?;
    let _ = writer.join();

    let code = match output.status.code() {
        Some(code) => code,
        // No exit code means fzf was killed by a signal; report 128 + signal (shell
        // convention) so a crash isn't silently indistinguishable from "no match".
        None => {
            #[cfg(unix)]
            {
                use std::os::unix::process::ExitStatusExt;
                output.status.signal().map_or(1, |sig| 128 + sig)
            }
            #[cfg(not(unix))]
            {
                1
            }
        }
    };
    if code != 0 {
        // 1 = no match, 130 = cancelled. Nothing selected.
        return Ok(code);
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let id = selected_slice_id(stdout.lines().next().unwrap_or(""));
    if id.is_empty() {
        return Ok(1);
    }
    if print {
        println!("{id}");
        Ok(0)
    } else {
        show(ctx, id, ShowMode::Metadata, false, styles)
    }
}

pub fn grep(
    ctx: &Context,
    selector: &str,
    pattern: &str,
    ignore_case: bool,
    fixed_strings: bool,
    symbols: bool,
) -> Result<i32> {
    let docs = load_slice_docs_meta(ctx)?;
    let doc = require_slice(&docs, selector)?;
    if doc.files.is_empty() {
        eprintln!("{} has no files[]", doc.slice_id);
        return Ok(1);
    }

    if symbols {
        return grep_with_symbols(ctx, &doc.files, pattern, ignore_case, fixed_strings);
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

/// `grep --symbols`: run `rg --json` so the path + line number come structurally (no fragile
/// `path:line:text` re-parse), then append a stable, uncolored `\t[span <Name> <start>-<end> approx]`
/// suffix naming the enclosing function/class. Unresolved/ambiguous hits are emitted bare. rg's exit
/// code (0 match / 1 none / 2 error) and stderr are preserved.
fn grep_with_symbols(
    ctx: &Context,
    files: &[String],
    pattern: &str,
    ignore_case: bool,
    fixed_strings: bool,
) -> Result<i32> {
    let mut command = Command::new("rg");
    command.arg("--json");
    if ignore_case {
        command.arg("-i");
    }
    if fixed_strings {
        command.arg("-F");
    }
    command.arg(pattern);
    for slice_pattern in files {
        let expanded = expand_literal_or_existing(slice_pattern, ctx);
        if expanded.is_empty() {
            command.arg(slice_pattern);
        } else {
            command.args(expanded);
        }
    }
    let output = match command.current_dir(ctx.repo_root()).output() {
        Ok(out) => out,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("rg is required for `slice grep`");
            return Ok(2);
        }
        Err(err) => return Err(Error::Io(err)),
    };
    if !output.stderr.is_empty() {
        eprint!("{}", String::from_utf8_lossy(&output.stderr));
    }

    let mut file_cache: FxHashMap<String, Option<String>> = FxHashMap::default();
    let stdout = io::stdout();
    let mut lock = stdout.lock();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let Ok(event) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if event.get("type").and_then(serde_json::Value::as_str) != Some("match") {
            continue;
        }
        let data = &event["data"];
        let Some(path) = data["path"]["text"].as_str() else {
            continue; // non-UTF8 path: skip rather than emit a malformed line
        };
        let lineno = usize::try_from(data["line_number"].as_u64().unwrap_or(0)).unwrap_or(0);
        let text = data["lines"]["text"].as_str().unwrap_or_default();
        let text = text.trim_end_matches(['\n', '\r']);

        // Resolve the optional enclosing-span annotation (each file read once, cached).
        let span = crate::symbols::lang_for_path(path).and_then(|lang| {
            let content = file_cache
                .entry(path.to_owned())
                .or_insert_with(|| std::fs::read_to_string(ctx.repo_root().join(path)).ok());
            content
                .as_ref()
                .and_then(|c| crate::symbols::enclosing_span(c, lineno, lang))
        });
        let written = match &span {
            Some(sym) => writeln!(
                lock,
                "{path}:{lineno}:{text}\t[span {} {}-{} approx]",
                sym.name, sym.start, sym.end
            ),
            None => writeln!(lock, "{path}:{lineno}:{text}"),
        };
        // A closed consumer (e.g. `slice grep --symbols … | head`) is a normal early exit, not a
        // CLI error — mirror rg's graceful handling instead of bubbling BrokenPipe up as exit 2.
        match written {
            Ok(()) => {}
            Err(err) if err.kind() == io::ErrorKind::BrokenPipe => return Ok(0),
            Err(err) => return Err(err.into()),
        }
    }
    Ok(output.status.code().unwrap_or(1))
}

pub fn docs(ctx: &Context, selector: &str, json: bool) -> Result<i32> {
    let slices = load_slice_docs_meta(ctx)?;
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

pub fn stale_docs(ctx: &Context, json: bool, styles: &Styles) -> Result<i32> {
    let docs = load_slice_docs_meta(ctx)?;
    let manifest = load_doc_manifest(ctx)?;
    let stale = stale_docs_for(ctx, &docs, &manifest.docs, StalenessMode::Attributed);
    let any_stale = !stale.is_empty();
    if json {
        emit_json(&stale)?;
    } else if stale.is_empty() {
        println!("all docs are up to date");
    } else {
        for doc in &stale {
            let since = doc.verified_at.chars().take(12).collect::<String>();
            println!(
                "{}  ({})  (since {})  [{}]",
                styles.paint(styles.stale, &doc.doc_id),
                styles.paint(styles.dim, &doc.path),
                since,
                doc.affected_slices.join(", ")
            );
            for file in &doc.changed_files {
                println!("  - {}", styles.paint(styles.dim, file));
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

    let slices = load_slice_docs_meta(ctx)?;
    let by_id = slices
        .iter()
        .map(|slice| (slice.slice_id.as_str(), slice))
        .collect::<FxHashMap<_, _>>();
    let target_ids = targets.into_iter().collect::<FxHashSet<_>>();
    let mut updated = Vec::with_capacity(manifest.docs.len());
    for mut doc in manifest.docs {
        if target_ids.contains(doc.doc_id.as_str()) {
            let concrete = resolve_tracked_files(&doc, &by_id, ctx)
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
            docs_root_raw: manifest.docs_root_raw,
            docs: updated,
        },
        ctx,
    )?;
    Ok(0)
}

struct DocsScan {
    docs_root: String,
    entries: BTreeMap<String, BootstrapEntry>,
    unresolved: Vec<(String, String)>,
    any_tracks: bool,
}

/// Scan a documentation directory: read each `.md` file's frontmatter and resolve its
/// `tracks:` code paths to owning slice IDs. Shared by `docs-bootstrap` and `init --docs`.
fn scan_docs_dir(ctx: &Context, docs_dir: &Path) -> Result<DocsScan> {
    let slices = load_slice_docs_meta(ctx)?;
    let docs_root = relative_path(docs_dir, ctx.slices_dir());
    let mut entries = BTreeMap::<String, BootstrapEntry>::new();
    let mut unresolved = Vec::<(String, String)>::new();
    let mut any_tracks = false;
    for md_file in markdown_files(docs_dir)? {
        let rel_path = md_file
            .strip_prefix(docs_dir)
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
            any_tracks = true;
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
    Ok(DocsScan {
        docs_root,
        entries,
        unresolved,
        any_tracks,
    })
}

fn scan_to_tracked_docs(entries: &BTreeMap<String, BootstrapEntry>) -> Vec<TrackedDoc> {
    entries
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
        .collect()
}

fn print_bootstrap_summary(ctx: &Context, scan: &DocsScan) {
    let mapped = scan
        .entries
        .values()
        .filter(|entry| !entry.slices.is_empty())
        .count();
    println!("wrote {}", ctx.rel(&ctx.docs_manifest_path()));
    println!("  docs:                {}", scan.entries.len());
    println!("  with slice mappings: {mapped}");
    println!(
        "  without mappings:    {}  (add `tracks:` to the doc and re-run, or stamp manually)",
        scan.entries.len() - mapped
    );
    if !scan.unresolved.is_empty() {
        println!("  unresolved tracks:   {}", scan.unresolved.len());
        for (doc_id, track) in scan.unresolved.iter().take(10) {
            println!("    [{doc_id}] {track}");
        }
        if scan.unresolved.len() > 10 {
            println!("    ... and {} more", scan.unresolved.len() - 10);
        }
    }
}

/// Bootstrap `slices/DOCS.yaml` from a documentation directory. Writes real doc→slice
/// mappings when any doc carries `tracks:` frontmatter; otherwise writes a commented
/// stub seeded with the docs it found. Refuses to clobber an existing manifest without
/// `--force`. Returns the exit code: 0 on success, 1 when a manifest already exists and
/// `--force` was not given, 2 when the docs directory is missing — so a typo'd path
/// fails loudly instead of silently leaving no manifest behind.
pub fn docs_bootstrap(ctx: &Context, docs_dir: &Path, dry_run: bool, force: bool) -> Result<i32> {
    // Resolve a relative docs dir against the repo root, not the process CWD, so
    // `slice --repo <repo> docs-bootstrap docs` works regardless of where it runs.
    let docs_dir = if docs_dir.is_absolute() {
        docs_dir.to_path_buf()
    } else {
        ctx.repo_root().join(docs_dir)
    };
    if !docs_dir.exists() {
        eprintln!("documentation directory not found: {}", ctx.rel(&docs_dir));
        return Ok(2);
    }
    let scan = scan_docs_dir(ctx, &docs_dir)?;
    if dry_run {
        print_bootstrap_dry_run(&scan.docs_root, &scan.entries, &scan.unresolved);
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
    if scan.any_tracks && !scan.entries.is_empty() {
        save_doc_manifest(
            &crate::models::DocManifest {
                docs_root_raw: Some(scan.docs_root.clone()),
                docs: scan_to_tracked_docs(&scan.entries),
            },
            ctx,
        )?;
        print_bootstrap_summary(ctx, &scan);
        println!("Run `slice stamp --all` to record baseline fingerprints.");
    } else {
        write_docs_stub(ctx, &scan)?;
        println!("wrote {} (stub)", ctx.rel(&manifest_path));
        println!("  Add `tracks: [<code paths>]` to each doc's frontmatter, then re-run");
        println!(
            "  `slice docs-bootstrap {} --force` and `slice stamp --all`.",
            ctx.rel(&docs_dir)
        );
    }
    Ok(0)
}

/// Write a commented DOCS.yaml stub, seeded with any docs found, when nothing carries
/// `tracks:` frontmatter yet. `save_doc_manifest` can't emit comments, so compose directly.
fn write_docs_stub(ctx: &Context, scan: &DocsScan) -> Result<()> {
    let mut content = String::new();
    content.push_str("# slice doc-staleness tracking. To enable it:\n");
    content.push_str(
        "#   1. add `tracks: [<code paths a doc describes>]` to each doc's frontmatter\n",
    );
    content.push_str("#   2. re-run `slice docs-bootstrap <dir> --force` to pick up the tracks\n");
    content.push_str("#   3. `slice stamp --all` to record baselines\n");
    content.push_str("docs_root: ");
    content.push_str(&scan.docs_root);
    content.push('\n');
    content.push_str("docs:\n");
    if scan.entries.is_empty() {
        content.push_str("  # example-doc:\n");
        content.push_str("  #   path: example-doc.md\n");
        content.push_str("  #   slices: [some-slice-id]\n");
    } else {
        for (doc_id, entry) in &scan.entries {
            content.push_str("  ");
            content.push_str(doc_id);
            content.push_str(":\n");
            content.push_str("    path: ");
            content.push_str(&entry.path);
            content.push('\n');
            content.push_str("    slices: []   # add the slice IDs this doc describes\n");
            content.push_str("    verified_at: \"\"\n");
        }
    }
    let path = ctx.docs_manifest_path();
    std::fs::write(&path, content).map_err(|source| Error::Write { path, source })
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

    let slices = load_slice_docs_meta(ctx)?;
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
    docs_root: &str,
    entries: &BTreeMap<String, BootstrapEntry>,
    unresolved: &[(String, String)],
) {
    println!("docs_root: {docs_root}");
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
        let tracked_files = resolve_tracked_files(doc, &by_id, ctx);
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
                let backend = ProcessGitBackend;
                match backend.changed_files(ctx, &tracked_files, &doc.verified_at) {
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
                let backend = ProcessGitBackend;
                match backend.changed_files(ctx, &tracked_files, &doc.verified_at) {
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
    // Split the needle into whitespace-separated terms and require EVERY term to hit some field
    // (per-term AND across fields), instead of matching the whole needle as one literal substring.
    // That lets natural-language queries like "Variable setitem values" match. An empty/whitespace
    // needle yields a single empty term, which is contained in every field — preserving the prior
    // match-all behavior. Quoted phrases do not combine; whitespace always splits.
    let mut terms: Vec<String> = needle
        .to_lowercase()
        .split_whitespace()
        .map(str::to_owned)
        .collect();
    if terms.is_empty() {
        terms.push(String::new());
    }

    let mut tags_by_slice = FxHashMap::<&str, Vec<&str>>::default();
    for tracked in tracked_docs {
        for sid in &tracked.slices {
            let tags = tags_by_slice.entry(sid.as_str()).or_default();
            tags.extend(tracked.tags.iter().map(String::as_str));
        }
    }

    let mut rows = Vec::new();
    for doc in docs {
        // Lower-case each field ONCE per doc (not once per term) so an N-term query over a large
        // body doesn't re-allocate it N times.
        let id_l = doc.slice_id.to_lowercase();
        let desc_l = doc.description.to_lowercase();
        let files_l: Vec<String> = doc.files.iter().map(|s| s.to_lowercase()).collect();
        let abs_l: Vec<String> = doc.abstractions.iter().map(|s| s.to_lowercase()).collect();
        let deps_l: Vec<String> = doc.dependencies.iter().map(|s| s.to_lowercase()).collect();
        let tags_l: Vec<String> = tags_by_slice
            .get(doc.slice_id.as_str())
            .map(|tags| tags.iter().map(|t| t.to_lowercase()).collect())
            .unwrap_or_default();
        let body_l = doc.body.to_lowercase();

        // Field order here is the public output order; keep it stable. Each field name is recorded
        // at most once even across multiple terms (dedup-preserving-order below).
        let mut fields: Vec<&str> = Vec::new();
        let mut all_terms_hit = true;
        for term in &terms {
            let checks: [(bool, &'static str); 7] = [
                (id_l.contains(term), "slice_id"),
                (desc_l.contains(term), "description"),
                (files_l.iter().any(|f| f.contains(term)), "files"),
                (abs_l.iter().any(|a| a.contains(term)), "abstractions"),
                (deps_l.iter().any(|d| d.contains(term)), "dependencies"),
                (tags_l.iter().any(|t| t.contains(term)), "doc_tags"),
                (body_l.contains(term), "body"),
            ];
            let mut term_hit = false;
            for (present, name) in checks {
                if present {
                    term_hit = true;
                    if !fields.contains(&name) {
                        fields.push(name);
                    }
                }
            }
            if !term_hit {
                all_terms_hit = false;
                break;
            }
        }
        if all_terms_hit && !fields.is_empty() {
            // Re-emit fields in the canonical order (closure pushes in first-hit order, which for a
            // single term already equals canonical order; this normalizes the multi-term case).
            let order = [
                "slice_id",
                "description",
                "files",
                "abstractions",
                "dependencies",
                "doc_tags",
                "body",
            ];
            let matches: Vec<&str> = order
                .into_iter()
                .filter(|name| fields.contains(name))
                .collect();
            rows.push(FindMatch {
                description: &doc.description,
                matches,
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

fn transitive_reverse_deps(start: &str, docs: &[SliceDoc]) -> Vec<String> {
    let reverse = reverse_deps(docs);
    let mut ordered = Vec::new();
    let mut seen = FxHashSet::default();
    // Seed with `start` so a dependency cycle can't list the slice itself as part
    // of its own blast radius.
    seen.insert(start.to_owned());
    let mut queue = VecDeque::from(reverse.get(start).cloned().unwrap_or_default());
    while let Some(current) = queue.pop_front() {
        if !seen.insert(current.clone()) {
            continue;
        }
        ordered.push(current.clone());
        if let Some(next) = reverse.get(current.as_str()) {
            for dep in next {
                if !seen.contains(dep) {
                    queue.push_back(dep.clone());
                }
            }
        }
    }
    ordered
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
    ctx: &Context,
) -> Vec<String> {
    let files = if doc.include.is_empty() {
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
    let mut files = files
        .into_iter()
        .flat_map(|file| {
            let expanded = expand_literal_or_existing(&file, ctx);
            if expanded.is_empty() {
                vec![file]
            } else {
                expanded
            }
        })
        .collect::<Vec<_>>();
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

/// The slice's lede: the prose summary before the first `## ` section. A leading
/// `# Title` H1 is dropped — slice files are generated with the title as an H1, which
/// is redundant with `slice_id` and looks odd in a non-rendered terminal.
fn slice_lede(body: &str) -> String {
    let mut lines = body.lines().peekable();
    if let Some(first) = lines.peek()
        && first.starts_with("# ")
        && !first.starts_with("## ")
    {
        lines.next();
    }
    let mut out = Vec::new();
    for line in lines {
        if line.starts_with("## ") {
            break;
        }
        out.push(line);
    }
    out.join("\n").trim().to_owned()
}

pub(crate) fn emit_json<T: Serialize>(value: &T) -> Result<()> {
    let stdout = io::stdout();
    let mut lock = stdout.lock();
    serde_json::to_writer_pretty(&mut lock, value)?;
    writeln!(&mut lock)?;
    Ok(())
}

/// A slice's source file read once: `(repo-relative path, language, contents)`.
type SliceSource = (String, crate::symbols::Lang, String);

/// Expand a slice's file patterns and read each supported source exactly once (deduped across
/// overlapping patterns), so abstraction lookup doesn't re-read files per abstraction.
fn read_slice_sources(ctx: &Context, files: &[String]) -> Vec<SliceSource> {
    let mut seen = FxHashSet::default();
    let mut out = Vec::new();
    for pattern in files {
        for rel in expand_literal_or_existing(pattern, ctx) {
            if !seen.insert(rel.clone()) {
                continue;
            }
            let Some(lang) = crate::symbols::lang_for_path(&rel) else {
                continue;
            };
            let Ok(content) = std::fs::read_to_string(repo_join(ctx, &rel)) else {
                continue;
            };
            out.push((rel, lang, content));
        }
    }
    out
}

/// Resolve an abstraction to `path:line` of its definition, or `None` when it can't be pinned
/// unambiguously. Requires the symbol to be defined in exactly one of the slice's files (a
/// duplicate or no definition → no location, never a guess). Reuses `normalize_abstraction` so the
/// name is parsed the same way `check` does.
fn abstraction_location(sources: &[SliceSource], abstraction: &str) -> Option<String> {
    let name = crate::check::normalize_abstraction(abstraction);
    if name.is_empty() {
        return None;
    }
    let mut hit: Option<String> = None;
    for (rel, lang, content) in sources {
        if let Some((start, _)) = crate::symbols::definition_span(content, &name, *lang) {
            if hit.is_some() {
                return None; // defined in more than one file → ambiguous
            }
            hit = Some(format!("{rel}:{start}"));
        }
    }
    hit
}

/// `abstractions:` block for `show`/`context`, with each item's definition site appended as a dim
/// `(path:line)` when it resolves unambiguously (human output only — no JSON field).
fn print_abstractions(ctx: &Context, files: &[String], abstractions: &[String], styles: &Styles) {
    let label = styles.paint(styles.dim, "abstractions:");
    if abstractions.is_empty() {
        println!("{label} (none)");
        return;
    }
    println!("{label}");
    let sources = read_slice_sources(ctx, files);
    for value in abstractions {
        match abstraction_location(&sources, value) {
            Some(loc) => println!(
                "  - {value}  {}",
                styles.paint(styles.dim, &format!("({loc})"))
            ),
            None => println!("  - {value}"),
        }
    }
}

fn print_list_colored(
    label: &str,
    values: &[String],
    styles: &Styles,
    value_style: Option<anstyle::Style>,
) {
    let label = styles.paint(styles.dim, &format!("{label}:"));
    if values.is_empty() {
        println!("{label} (none)");
    } else {
        println!("{label}");
        for value in values {
            match value_style {
                Some(style) => println!("  - {}", styles.paint(style, value)),
                None => println!("  - {value}"),
            }
        }
    }
}

fn print_tracked_docs(
    label: &str,
    values: &[&TrackedDoc],
    styles: &Styles,
    stale_ids: &FxHashSet<String>,
) {
    let header = styles.paint(styles.dim, &format!("{label}:"));
    if values.is_empty() {
        println!("{header} (none)");
    } else {
        println!("{header}");
        for value in values {
            let verified = if stale_ids.contains(&value.doc_id) {
                styles.paint(styles.stale, &value.verified_at)
            } else {
                value.verified_at.clone()
            };
            println!(
                "  - {{'doc_id': '{}', 'path': '{}', 'verified_at': '{}', 'tags': {}}}",
                value.doc_id,
                value.path,
                verified,
                python_list_repr(&value.tags)
            );
        }
    }
}

fn python_list_repr(values: &[String]) -> String {
    let items = values
        .iter()
        .map(|value| format!("'{value}'"))
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{items}]")
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{
        build_browse_rows, extract_sections, fzf_action, section_text, selected_slice_id,
        slice_lede,
    };
    use crate::color::{ColorChoice, Styles};
    use crate::models::{CollaboratorFile, SliceDoc};

    fn slice(id: &str, description: &str, loc: Option<u64>) -> SliceDoc {
        SliceDoc {
            slice_id: id.to_owned(),
            doc_path: PathBuf::from(format!("slices/{id}.md")),
            description: description.to_owned(),
            loc,
            files: Vec::new(),
            abstractions: Vec::new(),
            dependencies: Vec::new(),
            exclusions: Vec::new(),
            body: String::new(),
        }
    }

    #[test]
    fn collaborator_files_dedupes_and_preserves_distance_order() {
        let mut b = slice("chain-b", "B", None);
        b.files = vec!["src/b.rs".to_owned(), "src/shared.rs".to_owned()];
        let mut c = slice("chain-c", "C", None);
        c.files = vec!["src/shared.rs".to_owned(), "src/c.rs".to_owned()];
        let docs = vec![b, c];

        // dependency_ids arrive in distance order (nearest first): chain-b, then chain-c.
        let out = super::collaborator_files(&["chain-b".to_owned(), "chain-c".to_owned()], &docs);
        assert_eq!(
            out,
            vec![
                CollaboratorFile {
                    slice_id: "chain-b".to_owned(),
                    file: "src/b.rs".to_owned(),
                },
                // src/shared.rs is attributed to the NEARER slice (chain-b) and emitted once...
                CollaboratorFile {
                    slice_id: "chain-b".to_owned(),
                    file: "src/shared.rs".to_owned(),
                },
                // ...so chain-c contributes only its non-duplicate file. No cap drops it.
                CollaboratorFile {
                    slice_id: "chain-c".to_owned(),
                    file: "src/c.rs".to_owned(),
                },
            ]
        );

        // Empty blast radius → empty output (the caller turns this into an exit-0 empty list).
        assert!(super::collaborator_files(&[], &docs).is_empty());
        // An unknown dependency id is skipped, not an error.
        assert!(super::collaborator_files(&["ghost".to_owned()], &docs).is_empty());
    }

    #[test]
    fn browse_rows_show_id_first_padded_then_description() {
        let docs = vec![
            slice("auth", "Auth and sessions", Some(45)),
            slice("data-model", "Core types", None),
        ];
        let styles = Styles::resolve(ColorChoice::Never);
        let (rows, skipped) = build_browse_rows(&docs, &styles);
        assert!(skipped.is_empty());
        // ids padded to the widest ("data-model" = 10) so descriptions line up.
        assert_eq!(
            rows,
            "auth        Auth and sessions (45 LoC)\ndata-model  Core types\n"
        );
        // The id is the first whitespace token of a selected line.
        assert_eq!(
            selected_slice_id("auth        Auth and sessions (45 LoC)"),
            "auth"
        );
    }

    #[test]
    fn browse_rows_skip_ids_with_whitespace() {
        let docs = vec![
            slice("good", "ok", None),
            slice("bad id", "nope", None),
            slice("bad\tid", "nope", None),
        ];
        let styles = Styles::resolve(ColorChoice::Never);
        let (rows, skipped) = build_browse_rows(&docs, &styles);
        assert_eq!(rows, "good  ok\n");
        assert_eq!(skipped, vec!["bad id".to_owned(), "bad\tid".to_owned()]);
    }

    #[test]
    fn fzf_action_avoids_delimiter_collisions() {
        // No parens in the command → use the default ().
        assert_eq!(
            fzf_action("change-preview", "slice show {1}").unwrap(),
            "change-preview(slice show {1})"
        );
        // A ')' in the command forces a different delimiter than ().
        let with_paren = fzf_action("change-preview", "'/p)(q' show {1}").unwrap();
        assert!(!with_paren.starts_with("change-preview("));
        assert!(with_paren.starts_with("change-preview"));
        // Every candidate delimiter present → refuse rather than emit a broken bind.
        assert!(fzf_action("change-preview", "()[]<>~!@#%^|").is_err());
    }

    #[test]
    fn slice_lede_strips_h1_and_stops_at_first_section() {
        let body = "# Backend Auth\n\nThis slice owns auth.\n\nSecond paragraph.\n\n## Runtime Flows\n\na -> b\n";
        assert_eq!(
            slice_lede(body),
            "This slice owns auth.\n\nSecond paragraph."
        );
    }

    #[test]
    fn slice_lede_keeps_prose_when_no_h1() {
        // The mock auth-service shape: prose straight after frontmatter, no H1.
        let body = "Handles JWT verification.\n\n## System Behavior\n\nx\n";
        assert_eq!(slice_lede(body), "Handles JWT verification.");
    }

    #[test]
    fn slice_lede_is_empty_for_title_only_then_section() {
        assert_eq!(slice_lede("# Title\n\n## Runtime Flows\n\na -> b\n"), "");
    }

    #[test]
    fn slice_lede_is_empty_for_sections_only_or_empty_body() {
        assert_eq!(slice_lede("## System Behavior\n\nx\n"), "");
        assert_eq!(slice_lede(""), "");
    }

    #[test]
    fn section_extraction_parses_h2_sections() {
        let sections =
            extract_sections("intro\n\n## System Behavior\n\nRuns.\n\n## Verification\n\n- ok\n");
        assert_eq!(section_text(&sections, "System Behavior"), Some("Runs."));
        assert_eq!(section_text(&sections, "verification"), Some("- ok"));
    }

    #[test]
    fn section_extraction_ignores_h3_headings() {
        let sections = extract_sections("## System Behavior\n\n### Detail\ntext\n");
        assert_eq!(
            section_text(&sections, "System Behavior"),
            Some("### Detail\ntext")
        );
    }

    #[test]
    fn section_extraction_is_empty_without_h2_headings() {
        assert!(extract_sections("# Title\n\nbody").is_empty());
    }
}
