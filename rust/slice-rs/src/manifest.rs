use std::fs;

use rustc_hash::FxHashMap;
use serde::Deserialize;

use crate::context::Context;
use crate::models::{DocManifest, TrackedDoc};
use crate::{Error, Result};

#[derive(Debug, Deserialize)]
struct RawManifest {
    vault_root: Option<String>,
    docs: Option<FxHashMap<String, RawTrackedDoc>>,
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
    let mut docs = raw
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
    docs.sort_by(|a, b| a.doc_id.cmp(&b.doc_id));
    Ok(DocManifest {
        vault_root_raw: raw.vault_root,
        docs,
    })
}
