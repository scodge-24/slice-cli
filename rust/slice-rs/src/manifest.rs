use std::fs;

use indexmap::IndexMap;
use serde::Deserialize;

use crate::context::Context;
use crate::models::{DocManifest, TrackedDoc};
use crate::{Error, Result};

#[derive(Debug, Deserialize)]
struct RawManifest {
    vault_root: Option<String>,
    docs: Option<IndexMap<String, RawTrackedDoc>>,
}

#[derive(Debug, Deserialize)]
struct RawTrackedDoc {
    #[serde(default)]
    path: String,
    #[serde(default)]
    slices: Vec<String>,
    #[serde(default)]
    verified_at: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    include: Vec<String>,
    #[serde(default)]
    exclude: Vec<String>,
    #[serde(default)]
    fingerprint: String,
}

pub fn load_doc_manifest(ctx: &Context) -> Result<DocManifest> {
    let path = ctx.docs_manifest_path();
    if !path.exists() {
        return Ok(DocManifest {
            vault_root_raw: None,
            docs: Vec::new(),
        });
    }
    let rel = ctx.rel(&path);
    let content = fs::read_to_string(&path).map_err(|source| Error::Read {
        path: path.clone(),
        source,
    })?;
    let raw: RawManifest =
        yaml_serde::from_str(&content).map_err(|source| Error::Yaml { path: rel, source })?;
    let docs = raw
        .docs
        .unwrap_or_default()
        .into_iter()
        .map(|(doc_id, doc)| TrackedDoc {
            doc_id,
            path: doc.path,
            slices: doc.slices,
            verified_at: doc.verified_at,
            tags: doc.tags,
            include: doc.include,
            exclude: doc.exclude,
            fingerprint: doc.fingerprint,
        })
        .collect::<Vec<_>>();
    Ok(DocManifest {
        vault_root_raw: raw.vault_root,
        docs,
    })
}

pub fn save_doc_manifest(manifest: &DocManifest, ctx: &Context) -> Result<()> {
    let mut content = String::new();
    if let Some(vault_root) = &manifest.vault_root_raw {
        content.push_str("vault_root: ");
        content.push_str(vault_root);
        content.push('\n');
    }
    content.push_str("docs:\n");
    for doc in &manifest.docs {
        content.push_str("  ");
        content.push_str(&doc.doc_id);
        content.push_str(":\n");
        content.push_str("    path: ");
        content.push_str(&doc.path);
        content.push('\n');
        write_string_list(&mut content, "slices", &doc.slices);
        content.push_str("    verified_at: ");
        content.push_str(&doc.verified_at);
        content.push('\n');
        if !doc.fingerprint.is_empty() {
            content.push_str("    fingerprint: ");
            content.push_str(&doc.fingerprint);
            content.push('\n');
        }
        write_optional_string_list(&mut content, "tags", &doc.tags);
        write_optional_string_list(&mut content, "include", &doc.include);
        write_optional_string_list(&mut content, "exclude", &doc.exclude);
    }
    let path = ctx.docs_manifest_path();
    std::fs::write(&path, content).map_err(|source| Error::Write { path, source })
}

fn write_optional_string_list(content: &mut String, key: &str, values: &[String]) {
    if !values.is_empty() {
        write_string_list(content, key, values);
    }
}

fn write_string_list(content: &mut String, key: &str, values: &[String]) {
    content.push_str("    ");
    content.push_str(key);
    content.push_str(":\n");
    for value in values {
        content.push_str("    - ");
        content.push_str(value);
        content.push('\n');
    }
}
