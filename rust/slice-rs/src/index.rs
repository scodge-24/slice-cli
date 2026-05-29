use rustc_hash::{FxHashMap, FxHashSet};

use crate::context::Context;
use crate::models::SliceDoc;
use crate::paths::expand_literal_or_existing;
use crate::slices::load_slice_docs_meta;
use crate::{Error, Result};

#[must_use]
pub fn generate_index(docs: &[SliceDoc], ctx: &Context) -> String {
    let (_, current_order) = parse_index(ctx);
    let by_id = docs
        .iter()
        .map(|doc| (doc.slice_id.as_str(), doc))
        .collect::<FxHashMap<_, _>>();

    let mut ordered = Vec::new();
    let mut seen = FxHashSet::default();
    for sid in current_order {
        if let Some(doc) = by_id.get(sid.as_str()) {
            ordered.push(*doc);
            seen.insert(sid);
        }
    }

    let mut new_docs = docs
        .iter()
        .filter(|doc| !seen.contains(doc.slice_id.as_str()))
        .collect::<Vec<_>>();
    new_docs.sort_by(|a, b| a.slice_id.cmp(&b.slice_id));
    ordered.extend(new_docs);

    let mut lines = vec![
        "# Slice Index".to_owned(),
        String::new(),
        format!("Last updated: {}", ctx.head_sha()),
        format!("Source fingerprint: {}", source_fingerprint(docs, ctx)),
        String::new(),
        "| Slice ID | Description | LoC |".to_owned(),
        "|----------|-------------|-----|".to_owned(),
    ];
    for doc in ordered {
        lines.push(format!(
            "| `{}` | {} | {} |",
            doc.slice_id,
            doc.description,
            format_loc(doc.loc)
        ));
    }
    lines.push(String::new());
    lines.join("\n")
}

#[must_use]
pub fn source_fingerprint(docs: &[SliceDoc], ctx: &Context) -> String {
    let mut paths = FxHashSet::default();
    for doc in docs {
        paths.insert(ctx.rel(&doc.doc_path));
        for raw in &doc.files {
            paths.extend(expand_literal_or_existing(raw, ctx));
        }
    }
    let mut ordered = paths.into_iter().collect::<Vec<_>>();
    ordered.sort();
    crate::commands::content_fingerprint(ctx, &ordered)
}

pub fn sync_index(ctx: &Context, stdout: bool, check: bool) -> Result<i32> {
    let docs = load_slice_docs_meta(ctx)?;
    let content = generate_index(&docs, ctx);
    if stdout {
        print!("{content}");
        return Ok(0);
    }
    let index_path = ctx.index_path();
    if check {
        let current = std::fs::read_to_string(&index_path).unwrap_or_default();
        if current == content {
            println!("INDEX.md is in sync");
            return Ok(0);
        }
        eprintln!("INDEX.md is out of sync");
        return Ok(1);
    }
    std::fs::write(&index_path, content).map_err(|source| Error::Write {
        path: index_path.clone(),
        source,
    })?;
    println!("updated {}", ctx.rel(&index_path));
    Ok(0)
}

pub fn parse_index(ctx: &Context) -> (FxHashMap<String, IndexRow>, Vec<String>) {
    let mut rows = FxHashMap::default();
    let mut order = Vec::new();
    let Ok(content) = std::fs::read_to_string(ctx.index_path()) else {
        return (rows, order);
    };
    for line in content.lines() {
        let Some(rest) = line.strip_prefix("| `") else {
            continue;
        };
        let Some((slice_id, rest)) = rest.split_once("` |") else {
            continue;
        };
        let cols = rest
            .trim_matches('|')
            .split('|')
            .map(str::trim)
            .collect::<Vec<_>>();
        if cols.len() < 2 {
            continue;
        }
        rows.insert(
            slice_id.to_owned(),
            IndexRow {
                description: cols[0].to_owned(),
                loc: parse_loc(cols[1]),
            },
        );
        order.push(slice_id.to_owned());
    }
    (rows, order)
}

#[derive(Debug)]
pub struct IndexRow {
    pub description: String,
    pub loc: Option<u64>,
}

fn parse_loc(raw: &str) -> Option<u64> {
    let trimmed = raw.trim().trim_start_matches('~').replace(',', "");
    if trimmed == "?" {
        None
    } else {
        trimmed.parse().ok()
    }
}

fn format_loc(loc: Option<u64>) -> String {
    loc.map_or_else(|| "~?".to_owned(), |loc| format!("~{}", format_number(loc)))
}

fn format_number(value: u64) -> String {
    let raw = value.to_string();
    let mut out = String::with_capacity(raw.len() + raw.len() / 3);
    for (idx, ch) in raw.chars().rev().enumerate() {
        if idx > 0 && idx % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out.chars().rev().collect()
}
