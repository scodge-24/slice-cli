use std::fs;
use std::io::Read;

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
    load_slice_docs_inner(ctx, true)
}

pub fn load_slice_docs_meta(ctx: &Context) -> Result<Vec<SliceDoc>> {
    load_slice_docs_inner(ctx, false)
}

pub fn load_slice_body(ctx: &Context, doc: &SliceDoc) -> Result<String> {
    let rel = ctx.rel(&doc.doc_path);
    let (_, body) = read_slice_parts(&doc.doc_path, &rel, true)?;
    Ok(body)
}

fn load_slice_docs_inner(ctx: &Context, include_body: bool) -> Result<Vec<SliceDoc>> {
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
        let (frontmatter, body) = read_slice_parts(&path, &rel, include_body)?;
        let parsed = parse_slice_frontmatter(&frontmatter, &rel)?;
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
            body,
        });
    }
    Ok(docs)
}

fn parse_slice_frontmatter(frontmatter: &str, rel: &str) -> Result<SliceFrontmatter> {
    if let Some(parsed) = parse_slice_frontmatter_fast(frontmatter) {
        return Ok(parsed);
    }
    yaml_serde::from_str(frontmatter).map_err(|source| Error::Yaml {
        path: rel.to_owned(),
        source,
    })
}

fn parse_slice_frontmatter_fast(frontmatter: &str) -> Option<SliceFrontmatter> {
    let mut parsed = SliceFrontmatter {
        slice_id: String::new(),
        description: String::new(),
        loc: None,
        files: Vec::new(),
        abstractions: Vec::new(),
        dependencies: Vec::new(),
        exclusions: Vec::new(),
    };
    let mut list_key: Option<&str> = None;

    for raw_line in frontmatter.lines() {
        let line = raw_line.trim_end();
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some(key) = list_key {
            if let Some(item) = trimmed.strip_prefix("- ") {
                push_list_value(&mut parsed, key, strip_scalar(item));
                continue;
            }
            list_key = None;
        }
        if raw_line.starts_with(' ') {
            return None;
        }
        let (key, value) = line.split_once(':')?;
        let key = key.trim();
        let value = value.trim();
        match key {
            "slice_id" => parsed.slice_id = strip_scalar(value),
            "description" => parsed.description = strip_scalar(value),
            "loc" => {
                parsed.loc = match value {
                    "" | "null" | "~" => None,
                    value => Some(yaml_serde::Value::Number(yaml_serde::Number::from(
                        value.replace(',', "").parse::<u64>().ok()?,
                    ))),
                };
            }
            "files" | "abstractions" | "dependencies" | "exclusions" => {
                if value.is_empty() {
                    list_key = Some(key);
                } else if value != "[]" {
                    return None;
                }
            }
            _ => {}
        }
    }
    Some(parsed)
}

fn push_list_value(parsed: &mut SliceFrontmatter, key: &str, value: String) {
    match key {
        "files" => parsed.files.push(value),
        "abstractions" => parsed.abstractions.push(value),
        "dependencies" => parsed.dependencies.push(value),
        "exclusions" => parsed.exclusions.push(value),
        _ => {}
    }
}

fn strip_scalar(value: &str) -> String {
    let value = value.trim();
    if value.len() >= 2 {
        let bytes = value.as_bytes();
        if (bytes[0] == b'\'' && bytes[value.len() - 1] == b'\'')
            || (bytes[0] == b'"' && bytes[value.len() - 1] == b'"')
        {
            return value[1..value.len() - 1].to_owned();
        }
    }
    value.to_owned()
}

fn read_slice_parts(
    path: &std::path::Path,
    rel: &str,
    include_body: bool,
) -> Result<(String, String)> {
    const FRONTMATTER_READ_LIMIT: u64 = 4 * 1024;

    if include_body {
        let content = fs::read_to_string(path).map_err(|source| Error::Read {
            path: path.to_path_buf(),
            source,
        })?;
        let (frontmatter, body) =
            split_frontmatter(&content).ok_or_else(|| Error::MissingFrontmatter {
                path: rel.to_owned(),
            })?;
        return Ok((frontmatter.to_owned(), body.trim().to_owned()));
    }

    let file = fs::File::open(path).map_err(|source| Error::Read {
        path: path.to_path_buf(),
        source,
    })?;
    let mut head = String::new();
    file.take(FRONTMATTER_READ_LIMIT)
        .read_to_string(&mut head)
        .map_err(|source| Error::Read {
            path: path.to_path_buf(),
            source,
        })?;
    if let Some((frontmatter, _)) = split_frontmatter(&head) {
        return Ok((frontmatter.to_owned(), String::new()));
    }
    if u64::try_from(head.len()).unwrap_or(u64::MAX) < FRONTMATTER_READ_LIMIT {
        return Err(Error::MissingFrontmatter {
            path: rel.to_owned(),
        });
    }

    let content = fs::read_to_string(path).map_err(|source| Error::Read {
        path: path.to_path_buf(),
        source,
    })?;
    let (frontmatter, _) =
        split_frontmatter(&content).ok_or_else(|| Error::MissingFrontmatter {
            path: rel.to_owned(),
        })?;
    Ok((frontmatter.to_owned(), String::new()))
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
    use super::{coerce_u64, parse_slice_frontmatter_fast, split_frontmatter};

    #[test]
    fn splits_yaml_frontmatter() {
        let (fm, body) = split_frontmatter("---\na: b\n---\n# Body\n").unwrap();
        assert_eq!(fm, "a: b");
        assert_eq!(body, "# Body\n");
    }

    #[test]
    fn fast_parser_handles_slice_frontmatter_shape() {
        let parsed = parse_slice_frontmatter_fast(
            "slice_id: backend-meal-logs\n\
             description: 'Meal-log creation: idempotent path'\n\
             loc: 540\n\
             files:\n\
               - apps/backend/src/services/meal-logs.ts\n\
             abstractions:\n\
               - 'createMealLog - transactional write'\n\
             exclusions:\n\
               - apps/backend/src/services/meal-logs.test.ts\n\
             dependencies:\n\
               - backend-db\n\
               - shared-types\n",
        )
        .unwrap();

        assert_eq!(parsed.slice_id, "backend-meal-logs");
        assert_eq!(parsed.description, "Meal-log creation: idempotent path");
        assert_eq!(parsed.loc.as_ref().and_then(coerce_u64), Some(540));
        assert_eq!(parsed.files, vec!["apps/backend/src/services/meal-logs.ts"]);
        assert_eq!(
            parsed.abstractions,
            vec!["createMealLog - transactional write"]
        );
        assert_eq!(
            parsed.exclusions,
            vec!["apps/backend/src/services/meal-logs.test.ts"]
        );
        assert_eq!(parsed.dependencies, vec!["backend-db", "shared-types"]);
    }
}
