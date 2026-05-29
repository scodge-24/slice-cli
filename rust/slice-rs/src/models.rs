use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct SliceDoc {
    pub slice_id: String,
    pub doc_path: PathBuf,
    pub description: String,
    pub loc: Option<u64>,
    pub files: Vec<String>,
    pub abstractions: Vec<String>,
    pub dependencies: Vec<String>,
    pub exclusions: Vec<String>,
    pub body: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TrackedDoc {
    #[serde(skip)]
    pub doc_id: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub slices: Vec<String>,
    #[serde(default)]
    pub verified_at: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    #[serde(default)]
    pub fingerprint: String,
}

#[derive(Debug, Clone)]
pub struct DocManifest {
    pub vault_root_raw: Option<String>,
    pub docs: Vec<TrackedDoc>,
}

#[derive(Debug, Serialize)]
pub struct ListRow<'a> {
    pub description: &'a str,
    pub doc_count: usize,
    pub loc: Option<u64>,
    pub slice_id: &'a str,
}

#[derive(Debug, Serialize)]
pub struct SliceOwner<'a> {
    pub description: &'a str,
    pub slice_id: &'a str,
}

#[derive(Debug, Serialize)]
pub struct TrackedDocSummary<'a> {
    pub doc_id: &'a str,
    pub path: &'a str,
    pub tags: &'a [String],
    pub verified_at: &'a str,
}

#[derive(Debug, Serialize)]
pub struct ShowSlice<'a> {
    pub abstractions: &'a [String],
    pub dependencies: &'a [String],
    pub description: &'a str,
    pub doc_path: String,
    pub docs: Vec<TrackedDocSummary<'a>>,
    pub exclusions: &'a [String],
    pub files: &'a [String],
    pub loc: Option<u64>,
    pub slice_id: &'a str,
}

#[derive(Debug, Serialize)]
pub struct DepsOutput<'a> {
    pub dependencies: Vec<String>,
    pub mode: &'a str,
    pub slice_id: &'a str,
}

#[derive(Debug, Serialize)]
pub struct AffectedDoc {
    pub changed_files: Vec<String>,
    pub doc_id: String,
    pub matching_slices: Vec<String>,
    pub path: String,
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct StaleDoc {
    pub affected_slices: Vec<String>,
    pub changed_files: Vec<String>,
    pub doc_id: String,
    pub path: String,
    pub verified_at: String,
}

#[derive(Debug, Serialize)]
pub struct ContextOutput {
    pub slices: Vec<ContextSlice>,
}

#[derive(Debug, Serialize)]
pub struct ContextSlice {
    pub dependencies: Vec<String>,
    pub description: String,
    pub doc_path: String,
    pub docs: Vec<ContextDoc>,
    pub files: Vec<String>,
    pub sections: std::collections::BTreeMap<String, String>,
    pub slice_id: String,
}

#[derive(Debug, Serialize)]
pub struct ContextDoc {
    pub doc_id: String,
    pub path: String,
    pub stale: bool,
    pub verified_at: String,
}
