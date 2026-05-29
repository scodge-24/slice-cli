use std::fs;

use serde::Deserialize;

use crate::context::Context;
use crate::models::SliceDoc;
use crate::{Error, Result};

#[derive(Debug, Deserialize)]
struct SliceFrontmatter {
    #[serde(default)]
    slice_id: String,
    #[serde(default)]
    description: String,
    loc: Option<yaml_serde::Value>,
    #[serde(default)]
    files: Vec<String>,
    #[serde(default)]
    abstractions: Vec<String>,
    #[serde(default)]
    dependencies: Vec<String>,
    #[serde(default)]
    exclusions: Vec<String>,
}

pub fn load_slice_docs(ctx: &Context) -> Result<Vec<SliceDoc>> {
    let slices_dir = ctx.slices_dir();
    if !slices_dir.exists() {
        return Ok(Vec::new());
    }

    let mut paths = Vec::new();
    for entry in fs::read_dir(slices_dir)? {
        let path = entry?.path();
        if path.extension().is_some_and(|ext| ext == "md")
            && path.file_name().is_some_and(|name| name != "INDEX.md")
        {
            paths.push(path);
        }
    }
    paths.sort();

    let mut docs = Vec::with_capacity(paths.len());
    for path in paths {
        let rel = ctx.rel(&path);
        let content = fs::read_to_string(&path).map_err(|source| Error::Read {
            path: path.clone(),
            source,
        })?;
        let (frontmatter, body) = split_frontmatter(&content)
            .ok_or_else(|| Error::MissingFrontmatter { path: rel.clone() })?;
        let parsed: SliceFrontmatter =
            yaml_serde::from_str(frontmatter).map_err(|source| Error::Yaml {
                path: rel.clone(),
                source,
            })?;
        let slice_id = parsed.slice_id.trim().to_owned();
        if slice_id.is_empty() {
            return Err(Error::MissingSliceId { path: rel });
        }
        docs.push(SliceDoc {
            slice_id,
            doc_path: path,
            description: parsed.description.trim().to_owned(),
            loc: parsed.loc.as_ref().and_then(coerce_u64),
            files: parsed.files,
            abstractions: parsed.abstractions,
            dependencies: parsed.dependencies,
            exclusions: parsed.exclusions,
            body: body.trim().to_owned(),
        });
    }
    Ok(docs)
}

#[must_use]
pub fn slice_for_selector<'a>(docs: &'a [SliceDoc], selector: &str) -> Option<&'a SliceDoc> {
    let normalized = selector.trim().trim_end_matches(".md");
    docs.iter().find(|doc| {
        doc.slice_id == normalized
            || doc
                .doc_path
                .file_stem()
                .is_some_and(|stem| stem == normalized)
    })
}

#[must_use]
pub fn docs_for_slice<'a>(
    tracked_docs: &'a [crate::models::TrackedDoc],
    slice_id: &str,
) -> Vec<&'a crate::models::TrackedDoc> {
    tracked_docs
        .iter()
        .filter(|doc| doc.slices.iter().any(|sid| sid == slice_id))
        .collect()
}

#[must_use]
pub fn owners_for_path<'a>(
    docs: &'a [SliceDoc],
    raw_path: &str,
    ctx: &Context,
) -> Vec<&'a SliceDoc> {
    let normalized = crate::paths::normalize_repo_path(raw_path, ctx);
    docs.iter()
        .filter(|doc| {
            doc.files
                .iter()
                .any(|pattern| crate::paths::matches_path(&normalized, pattern))
        })
        .collect()
}

fn split_frontmatter(content: &str) -> Option<(&str, &str)> {
    let rest = content.strip_prefix("---\n")?;
    let end = rest.find("\n---")?;
    let frontmatter = &rest[..end];
    let body_start = end + "\n---".len();
    let body = rest[body_start..]
        .strip_prefix('\n')
        .unwrap_or(&rest[body_start..]);
    Some((frontmatter, body))
}

fn coerce_u64(value: &yaml_serde::Value) -> Option<u64> {
    match value {
        yaml_serde::Value::Number(n) => n.as_u64(),
        yaml_serde::Value::String(s) => s.replace(',', "").parse().ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::split_frontmatter;

    #[test]
    fn splits_yaml_frontmatter() {
        let (fm, body) = split_frontmatter("---\na: b\n---\n# Body\n").unwrap();
        assert_eq!(fm, "a: b");
        assert_eq!(body, "# Body\n");
    }
}
