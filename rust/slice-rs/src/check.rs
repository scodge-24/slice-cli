use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;
use std::process::Command;

use rustc_hash::{FxHashMap, FxHashSet};
use serde::Serialize;

use crate::Result;
use crate::commands::{StalenessMode, stale_docs_for};
use crate::context::Context;
use crate::index::{parse_index, source_fingerprint};
use crate::manifest::load_doc_manifest;
use crate::models::SliceDoc;
use crate::paths::{expand_literal_or_existing, matches_path};
use crate::slices::load_slice_docs;

const SOURCE_EXTENSIONS: [&str; 29] = [
    ".py", ".ts", ".tsx", ".js", ".jsx", ".go", ".rs", ".rb", ".java", ".kt", ".cs", ".c", ".cpp",
    ".h", ".hpp", ".swift", ".vue", ".svelte", ".ex", ".exs", ".erl", ".zig", ".lua", ".php",
    ".scala", ".clj", ".hs", ".ml", ".mli",
];

#[expect(clippy::struct_excessive_bools, reason = "mirrors CLI check flags")]
#[derive(Debug, Clone, Copy)]
pub struct CheckOptions {
    pub strict_index: bool,
    pub staleness: bool,
    pub staged_coverage: bool,
    pub doc_drift: bool,
    pub require_verification: bool,
}

#[derive(Debug, Default)]
pub struct CheckResult {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub hidden_warnings: Vec<String>,
}

impl CheckResult {
    #[must_use]
    pub fn ok(&self) -> bool {
        self.errors.is_empty()
    }
}

#[derive(Debug, Serialize)]
pub struct CheckOutput {
    pub errors: Vec<String>,
    pub hidden_warning_categories: BTreeMap<String, usize>,
    pub hidden_warning_count: usize,
    pub hidden_warnings: Vec<String>,
    pub ok: bool,
    pub slice_count: usize,
    pub strict_index: bool,
    pub warnings: Vec<String>,
}

pub fn run(ctx: &Context, options: CheckOptions) -> Result<(usize, CheckResult)> {
    let docs = load_slice_docs(ctx)?;
    let mut result = CheckResult::default();
    let by_id = docs
        .iter()
        .map(|doc| (doc.slice_id.as_str(), doc))
        .collect::<FxHashMap<_, _>>();
    structural_checks(
        ctx,
        &docs,
        &by_id,
        options.require_verification,
        &mut result,
    );
    overlap_checks(ctx, &docs, &mut result);
    index_checks(ctx, &docs, options, &mut result);
    if options.staged_coverage {
        staged_coverage_checks(ctx, &docs, &mut result);
    }
    if options.doc_drift {
        doc_manifest_checks(ctx, &docs, &by_id, &mut result)?;
    }
    Ok((docs.len(), result))
}

#[must_use]
pub fn output(slice_count: usize, result: CheckResult, strict_index: bool) -> CheckOutput {
    let mut categories = BTreeMap::new();
    for warning in &result.hidden_warnings {
        *categories
            .entry(warning_category(warning).to_owned())
            .or_insert(0) += 1;
    }
    CheckOutput {
        ok: result.ok(),
        slice_count,
        errors: result.errors,
        warnings: result.warnings,
        hidden_warning_count: result.hidden_warnings.len(),
        hidden_warning_categories: categories,
        hidden_warnings: result.hidden_warnings,
        strict_index,
    }
}

fn structural_checks(
    ctx: &Context,
    docs: &[SliceDoc],
    by_id: &FxHashMap<&str, &SliceDoc>,
    require_verification: bool,
    result: &mut CheckResult,
) {
    let mut seen = FxHashSet::default();
    for doc in docs {
        if !seen.insert(doc.slice_id.as_str()) {
            result
                .errors
                .push(format!("duplicate slice_id: {}", doc.slice_id));
        }
        let doc_rel = ctx.rel(&doc.doc_path);
        if doc.description.is_empty() {
            result
                .errors
                .push(format!("{doc_rel}: missing description"));
        }
        if doc.loc.is_none() {
            result
                .warnings
                .push(format!("{doc_rel}: missing or non-numeric loc"));
        }
        if doc.files.is_empty() {
            result
                .warnings
                .push(format!("{doc_rel}: no files[] entries"));
        }
        for raw in &doc.files {
            if has_glob_meta(raw) {
                if expand_literal_or_existing(raw, ctx).is_empty() {
                    result
                        .errors
                        .push(format!("{doc_rel}: glob matches nothing: {raw}"));
                }
            } else if !ctx.repo_root().join(raw).exists() {
                result
                    .errors
                    .push(format!("{doc_rel}: file missing: {raw}"));
            }
        }
        if doc
            .doc_path
            .file_stem()
            .is_some_and(|stem| stem != doc.slice_id.as_str())
        {
            let stem = doc
                .doc_path
                .file_stem()
                .map_or_else(String::new, |stem| stem.to_string_lossy().into_owned());
            result.errors.push(format!(
                "{doc_rel}: slice_id '{}' != filename '{stem}'",
                doc.slice_id
            ));
        }
        for dep in &doc.dependencies {
            if !by_id.contains_key(dep.as_str()) && !dep.starts_with("external:") {
                result
                    .errors
                    .push(format!("{doc_rel}: unknown dependency: {dep}"));
            }
        }
        verification_checks(ctx, doc, require_verification, result);
    }
}

fn verification_checks(
    ctx: &Context,
    doc: &SliceDoc,
    require_verification: bool,
    result: &mut CheckResult,
) {
    let links = parse_verification(&doc.body);
    let upstream = parse_upstream(&doc.body);
    let doc_rel = ctx.rel(&doc.doc_path);
    for (_, refs) in &links {
        for reference in refs {
            let ref_file = reference
                .split_once("::")
                .map_or(reference.as_str(), |(file, _)| file);
            if !ctx.repo_root().join(ref_file).exists() {
                result
                    .errors
                    .push(format!("{doc_rel}: verification ref missing: {reference}"));
            }
        }
    }
    for upstream_doc in upstream {
        if !ctx.repo_root().join(&upstream_doc).exists() {
            result.errors.push(format!(
                "{doc_rel}: verification upstream missing: {upstream_doc}"
            ));
        }
    }
    if require_verification && !doc.abstractions.is_empty() {
        let linked = links
            .iter()
            .map(|(abstraction, _)| normalize_abstraction(abstraction))
            .collect::<FxHashSet<_>>();
        for raw in &doc.abstractions {
            let name = normalize_abstraction(raw);
            if !name.is_empty() && !linked.contains(name.as_str()) {
                result.errors.push(format!(
                    "{doc_rel}: abstraction not verified: {name} \
                     - add `- `{name}` <- path/to/test::test_name` under ## Verification, \
                     or drop {name} from `abstractions:` if untested \
                     (see docs/verification-links.md)"
                ));
            }
        }
    }
}

fn overlap_checks(ctx: &Context, docs: &[SliceDoc], result: &mut CheckResult) {
    let mut owners = FxHashMap::<String, &str>::default();
    for doc in docs {
        for raw in &doc.files {
            for rel in expand_literal_or_existing(raw, ctx) {
                if !ctx.repo_root().join(&rel).is_file() {
                    continue;
                }
                if let Some(existing) = owners.insert(rel.clone(), &doc.slice_id) {
                    result.errors.push(format!(
                        "file overlap: {rel} in '{existing}' and '{}'",
                        doc.slice_id
                    ));
                }
            }
        }
    }
}

fn index_checks(ctx: &Context, docs: &[SliceDoc], options: CheckOptions, result: &mut CheckResult) {
    let (rows, _) = parse_index(ctx);
    let doc_ids = docs
        .iter()
        .map(|doc| doc.slice_id.as_str())
        .collect::<FxHashSet<_>>();
    let index_ids = rows.keys().map(String::as_str).collect::<FxHashSet<_>>();
    let mut missing = doc_ids.difference(&index_ids).copied().collect::<Vec<_>>();
    missing.sort_unstable();
    if !missing.is_empty() {
        result
            .errors
            .push(format!("INDEX.md missing rows: {}", missing.join(", ")));
    }
    let mut extra = index_ids.difference(&doc_ids).copied().collect::<Vec<_>>();
    extra.sort_unstable();
    if !extra.is_empty() {
        result
            .errors
            .push(format!("INDEX.md stale rows: {}", extra.join(", ")));
    }
    for doc in docs {
        let Some(row) = rows.get(&doc.slice_id) else {
            continue;
        };
        if row.description != doc.description {
            result
                .hidden_warnings
                .push(format!("INDEX.md description drift for {}", doc.slice_id));
        }
        if doc.loc.is_some() && row.loc != doc.loc {
            result
                .hidden_warnings
                .push(format!("INDEX.md loc drift for {}", doc.slice_id));
        }
    }
    if options.strict_index {
        result.warnings.extend(result.hidden_warnings.clone());
    }
    if options.staleness && ctx.index_path().is_file() {
        index_staleness(ctx, docs, result);
    }
}

fn index_staleness(ctx: &Context, docs: &[SliceDoc], result: &mut CheckResult) {
    let content = std::fs::read_to_string(ctx.index_path()).unwrap_or_default();
    let recorded = find_after(&content, "Source fingerprint:")
        .or_else(|| find_after(&content, "Last updated:"));
    if recorded.is_none() {
        result
            .warnings
            .push("INDEX.md has no 'Last updated: <hash>' line".to_owned());
        return;
    }
    let recorded = recorded.unwrap_or_default();
    let current = source_fingerprint(docs, ctx);
    if !fingerprint_equal(&recorded, &current) {
        result.warnings.push(format!(
            "INDEX.md stale: recorded {}, source fingerprint is {}",
            &recorded[..recorded.len().min(12)],
            &current[..current.len().min(12)]
        ));
    }
}

fn staged_coverage_checks(ctx: &Context, docs: &[SliceDoc], result: &mut CheckResult) {
    let Ok(output) = Command::new("git")
        .args(["diff", "--cached", "--name-only", "--diff-filter=ACMR"])
        .current_dir(ctx.repo_root())
        .output()
    else {
        return;
    };
    let staged = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| {
            !line.is_empty()
                && Path::new(line)
                    .extension()
                    .and_then(std::ffi::OsStr::to_str)
                    .is_some_and(|ext| SOURCE_EXTENSIONS.contains(&format!(".{ext}").as_str()))
        })
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    if staged.is_empty() {
        return;
    }
    let mut coverage = FxHashSet::default();
    let mut glob_patterns = Vec::new();
    for doc in docs {
        for raw in &doc.files {
            if has_glob_meta(raw) {
                glob_patterns.push(raw.as_str());
            }
            coverage.extend(expand_literal_or_existing(raw, ctx));
        }
    }
    for rel in staged {
        if !coverage.contains(rel.as_str())
            && !glob_patterns
                .iter()
                .any(|pattern| matches_path(&rel, pattern))
        {
            result
                .warnings
                .push(format!("staged file uncovered: {rel}"));
        }
    }
}

fn doc_manifest_checks(
    ctx: &Context,
    docs: &[SliceDoc],
    by_id: &FxHashMap<&str, &SliceDoc>,
    result: &mut CheckResult,
) -> Result<()> {
    let manifest = load_doc_manifest(ctx)?;
    if manifest.docs.is_empty() {
        return Ok(());
    }
    let docs_root = manifest
        .docs_root_raw
        .as_ref()
        .map(|raw| ctx.slices_dir().join(raw));
    for doc in &manifest.docs {
        if let Some(docs_root) = &docs_root {
            let path = docs_root.join(&doc.path);
            if path.exists() {
                match frontmatter_doc_id(&path) {
                    None => result.errors.push(format!(
                        "DOCS.yaml: {}: doc has no doc_id in frontmatter",
                        doc.doc_id
                    )),
                    Some(found) if found != doc.doc_id => result.errors.push(format!(
                        "DOCS.yaml: manifest key '{}' != frontmatter doc_id '{}' in {}",
                        doc.doc_id, found, doc.path
                    )),
                    Some(_) => {}
                }
            } else {
                result.errors.push(format!(
                    "DOCS.yaml: doc missing: {} ({})",
                    doc.doc_id, doc.path
                ));
            }
        }
        for sid in &doc.slices {
            if !by_id.contains_key(sid.as_str()) {
                result.errors.push(format!(
                    "DOCS.yaml: {} references unknown slice: {sid}",
                    doc.doc_id
                ));
            }
        }
    }
    for drift in stale_docs_for(ctx, docs, &manifest.docs, StalenessMode::Attributed) {
        result.warnings.push(format!(
            "doc stale: {} (verified_at: {}, slices: {}, changed: {})",
            drift.doc_id,
            &drift.verified_at[..drift.verified_at.len().min(12)],
            summarize(&drift.affected_slices),
            summarize(&drift.changed_files)
        ));
    }
    Ok(())
}

fn parse_verification(body: &str) -> Vec<(String, Vec<String>)> {
    let section = section_text(body, "Verification");
    let mut links = Vec::new();
    for line in section.lines() {
        let item = line.trim().strip_prefix("- ").unwrap_or("").trim();
        if item.to_lowercase().starts_with("upstream:") || !item.contains(" <- ") {
            continue;
        }
        let (left, right) = item.split_once(" <- ").unwrap_or(("", ""));
        let refs = right
            .split(',')
            .map(|ref_text| ref_text.trim().trim_matches('`').to_owned())
            .filter(|ref_text| !ref_text.is_empty())
            .collect::<Vec<_>>();
        let abstraction = left.trim().trim_matches('`').to_owned();
        if !abstraction.is_empty() && !refs.is_empty() {
            links.push((abstraction, refs));
        }
    }
    links
}

fn parse_upstream(body: &str) -> Vec<String> {
    let section = section_text(body, "Verification");
    let mut out = Vec::new();
    for line in section.lines() {
        let item = line.trim().strip_prefix("- ").unwrap_or("").trim();
        if let Some(rest) = item
            .strip_prefix("upstream:")
            .or_else(|| item.strip_prefix("Upstream:"))
        {
            out.extend(
                rest.split(',')
                    .map(|path| path.trim().trim_matches('`').to_owned())
                    .filter(|path| !path.is_empty()),
            );
        }
    }
    out
}

fn section_text(body: &str, name: &str) -> String {
    let mut current: Option<String> = None;
    let mut buffer = Vec::new();
    for line in body.lines() {
        if let Some(heading) = line.strip_prefix("## ") {
            if current
                .as_deref()
                .is_some_and(|current| current.eq_ignore_ascii_case(name))
            {
                return buffer.join("\n").trim_matches('\n').to_owned();
            }
            current = Some(heading.trim().to_owned());
            buffer.clear();
        } else if current.is_some() {
            buffer.push(line);
        }
    }
    if current
        .as_deref()
        .is_some_and(|current| current.eq_ignore_ascii_case(name))
    {
        buffer.join("\n").trim_matches('\n').to_owned()
    } else {
        String::new()
    }
}

fn normalize_abstraction(raw: &str) -> String {
    let mut name = raw.trim().trim_matches('`');
    if let Some((head, _)) = name.split_once('—') {
        name = head;
    } else if let Some((head, _)) = name.split_once(" - ") {
        name = head;
    }
    name.trim().trim_matches('`').to_owned()
}

fn frontmatter_doc_id(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let frontmatter = text
        .strip_prefix("---\n")
        .and_then(|rest| rest.find("\n---").map(|end| &rest[..end]))?;
    let value: yaml_serde::Value = yaml_serde::from_str(frontmatter).ok()?;
    let yaml_serde::Value::Mapping(mapping) = value else {
        return None;
    };
    for (key, value) in mapping {
        if key == yaml_serde::Value::String("doc_id".to_owned())
            && let yaml_serde::Value::String(doc_id) = value
        {
            return Some(doc_id);
        }
    }
    None
}

fn warning_category(message: &str) -> &str {
    if message.contains("description drift") {
        "index_description_drift"
    } else if message.contains("loc drift") {
        "index_loc_drift"
    } else {
        "other"
    }
}

fn has_glob_meta(raw: &str) -> bool {
    raw.bytes().any(|b| matches!(b, b'*' | b'?' | b'['))
}

fn fingerprint_equal(recorded: &str, current: &str) -> bool {
    if recorded == current {
        return true;
    }
    if recorded.len() <= 40 && current.len() <= 40 {
        let shared = recorded.len().min(current.len());
        shared > 0 && recorded[..shared] == current[..shared]
    } else {
        false
    }
}

fn find_after(content: &str, prefix: &str) -> Option<String> {
    content.lines().find_map(|line| {
        line.trim()
            .strip_prefix(prefix)
            .map(str::trim)
            .and_then(|rest| rest.split_whitespace().next())
            .map(ToOwned::to_owned)
    })
}

fn summarize(values: &[String]) -> String {
    let mut text = values
        .iter()
        .take(3)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    if values.len() > 3 {
        let _ = write!(text, " (+{} more)", values.len() - 3);
    }
    text
}

#[cfg(test)]
mod tests {
    use super::{normalize_abstraction, parse_upstream, parse_verification};

    #[test]
    fn verification_parser_extracts_links_and_upstream_refs() {
        let body = "## Verification\n\n- `verify_token` <- tests/test_auth.py::test_valid, `tests/test_auth.py::test_empty`\n- upstream: docs/auth.md, `docs/design.md`\n";
        let links = parse_verification(body);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].0, "verify_token");
        assert_eq!(
            links[0].1,
            vec![
                "tests/test_auth.py::test_valid",
                "tests/test_auth.py::test_empty"
            ]
        );
        assert_eq!(parse_upstream(body), vec!["docs/auth.md", "docs/design.md"]);
    }

    #[test]
    fn verification_parser_ignores_freetext() {
        let body = "## Verification\n\nThis is prose.\n- no arrow here\n- upstream:\n";
        assert!(parse_verification(body).is_empty());
        assert!(parse_upstream(body).is_empty());
    }

    #[test]
    fn normalize_abstraction_removes_descriptive_suffix() {
        assert_eq!(
            normalize_abstraction("`verify_token — JWT verification`"),
            "verify_token"
        );
        assert_eq!(
            normalize_abstraction("create_session - makes a session"),
            "create_session"
        );
    }

    #[test]
    fn verification_symbol_part_is_not_validated_as_a_file() {
        let body = "## Verification\n\n- `verify_token` <- tests/test_auth.py::missing_symbol\n";
        let links = parse_verification(body);
        let file = links[0].1[0].split_once("::").unwrap().0;
        assert_eq!(file, "tests/test_auth.py");
    }
}
