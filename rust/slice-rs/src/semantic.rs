//! Stage 4 — semantic / hybrid retrieval lens. Compiled only under the `semantic` feature.
//!
//! Embeds slice-ANCHORED units (slice descriptions and abstractions today; runtime-flow and
//! test-description units can be added to [`extract_units`] later without touching anything else) and
//! persists a staleness-tracked vector index as slice-owned state (`slices/SEMANTIC.json`). This
//! file is commit 1: build + persist the index. The query path (`find --semantic`) lands next and
//! treats the vector score as a CANDIDATE GENERATOR only — deterministic topology owns the final
//! rank — and flags any hit whose owning slice has drifted since the index was built.
//!
//! Plan §7 guardrails honoured here and below: embed only slice-anchored units, never anonymous
//! source chunks; the provider is a trait (`OpenRouter` first, a local/offline backend droppable in
//! later); every unit stores its owning slice's content fingerprint so a stale hit can be flagged,
//! not silently returned.

use serde::{Deserialize, Serialize};

use crate::commands::content_fingerprint;
use crate::context::Context;
use crate::models::SliceDoc;
use crate::paths::expand_literal_or_existing;
use crate::slices::load_slice_docs;
use crate::{Error, Result};

const DEFAULT_MODEL: &str = "openai/text-embedding-3-small";
/// text-embedding-3-* support dimension reduction; 512 keeps the on-disk index lean with minimal
/// quality loss versus the native 1536.
const DEFAULT_DIMS: usize = 512;
const OPENROUTER_URL: &str = "https://openrouter.ai/api/v1/embeddings";
/// Slice-owned artifact, sibling to `DOCS.yaml` / `INDEX.md`.
pub const INDEX_FILE: &str = "SEMANTIC.json";

/// An embedding provider. `OpenRouter` is the first backend; a local/offline embedder can implement
/// the same trait later without touching the index or query code (plan §7: pluggable, ships local-able).
pub trait Embedder {
    /// Embed each input text, returning one vector per input in the same order.
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;
    /// The model identifier recorded in the index for provenance/staleness.
    fn model_id(&self) -> &str;
}

/// `OpenAI`-compatible embeddings over `OpenRouter` (`/api/v1/embeddings`). Reads the API key from
/// `OPENROUTER_API_KEY`; the key is only ever placed in the request header, never logged or returned.
pub struct OpenRouterEmbedder {
    api_key: String,
    model: String,
    dims: usize,
}

impl OpenRouterEmbedder {
    pub fn from_env(model: String, dims: usize) -> Result<Self> {
        let api_key = std::env::var("OPENROUTER_API_KEY")
            .map_err(|_| Error::Embedding("OPENROUTER_API_KEY is not set".to_owned()))?;
        Ok(Self {
            api_key,
            model,
            dims,
        })
    }
}

impl Embedder for OpenRouterEmbedder {
    fn model_id(&self) -> &str {
        &self.model
    }

    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let mut out = Vec::with_capacity(texts.len());
        // Chunk so a large slice corpus never builds one oversized request.
        for chunk in texts.chunks(128) {
            let body = serde_json::json!({
                "model": self.model,
                "input": chunk,
                "dimensions": self.dims,
            });
            let resp = ureq::post(OPENROUTER_URL)
                .set("Authorization", &format!("Bearer {}", self.api_key))
                .set("Content-Type", "application/json")
                .send_json(body)
                // ureq Status/Transport errors carry the URL + status, never the auth header.
                .map_err(|e| Error::Embedding(e.to_string()))?;
            let parsed: EmbeddingResponse = resp
                .into_json()
                .map_err(|e| Error::Embedding(e.to_string()))?;
            let mut items = parsed.data;
            items.sort_by_key(|d| d.index); // OpenAI returns items keyed by request index
            for d in items {
                out.push(d.embedding);
            }
        }
        if out.len() != texts.len() {
            return Err(Error::Embedding(format!(
                "embedding count mismatch: got {}, expected {}",
                out.len(),
                texts.len()
            )));
        }
        Ok(out)
    }
}

#[derive(Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingDatum>,
}

#[derive(Deserialize)]
struct EmbeddingDatum {
    index: usize,
    embedding: Vec<f32>,
}

/// The persisted index. Provider/model/dims are recorded so the query path can reject a mismatch
/// (you cannot compare vectors from a different model), and `built_at` is a human-readable HEAD note.
#[derive(Serialize, Deserialize)]
pub struct SemanticIndex {
    pub provider: String,
    pub model: String,
    pub dims: usize,
    pub built_at: String,
    pub units: Vec<IndexUnit>,
}

/// One embedded slice-anchored unit. `slice_fp` is the owning slice's content fingerprint at build
/// time — the query path recomputes it and flags the hit stale if it has drifted.
#[derive(Serialize, Deserialize)]
pub struct IndexUnit {
    pub slice_id: String,
    pub kind: String,
    pub anchor: Option<String>,
    pub text: String,
    pub slice_fp: String,
    pub vector: Vec<f32>,
}

struct PendingUnit {
    slice_id: String,
    kind: &'static str,
    anchor: Option<String>,
    text: String,
    slice_fp: String,
}

/// The owning slice's content fingerprint: its card plus every source file it covers, hashed with the
/// same machinery `DOCS.yaml` staleness uses. Drift here = the unit's embedding may no longer reflect
/// the code, which is what the query path flags.
fn slice_fingerprint(ctx: &Context, doc: &SliceDoc) -> String {
    let mut paths = vec![ctx.rel(&doc.doc_path)];
    for raw in &doc.files {
        paths.extend(expand_literal_or_existing(raw, ctx));
    }
    paths.sort();
    paths.dedup();
    content_fingerprint(ctx, &paths)
}

/// The slice-anchored `(kind, text)` units for one slice — pure, no IO. Today: the description (its
/// natural-language summary) and each abstraction (its symbol-level concepts). Both are derived,
/// slice-owned text — never anonymous source. Runtime-flow and test-description units belong here too
/// and can be added without changing the build or query paths.
fn unit_texts(doc: &SliceDoc) -> Vec<(&'static str, String)> {
    let mut out = Vec::new();
    if !doc.description.trim().is_empty() {
        out.push(("description", doc.description.clone()));
    }
    for abs in &doc.abstractions {
        if !abs.trim().is_empty() {
            out.push(("abstraction", abs.clone()));
        }
    }
    out
}

/// Attach each slice's fingerprint to its [`unit_texts`], producing the units to embed.
fn extract_units(ctx: &Context, docs: &[SliceDoc]) -> Vec<PendingUnit> {
    let mut units = Vec::new();
    for doc in docs {
        let fp = slice_fingerprint(ctx, doc);
        for (kind, text) in unit_texts(doc) {
            units.push(PendingUnit {
                slice_id: doc.slice_id.clone(),
                kind,
                anchor: None,
                text,
                slice_fp: fp.clone(),
            });
        }
    }
    units
}

/// Build and persist the semantic index. Network-bound (one embedding call per ≤128-unit chunk).
pub fn build_index(ctx: &Context, model: Option<String>, dimensions: Option<usize>) -> Result<i32> {
    let model = model.unwrap_or_else(|| DEFAULT_MODEL.to_owned());
    let dims = dimensions.unwrap_or(DEFAULT_DIMS);
    let docs = load_slice_docs(ctx)?;
    let pending = extract_units(ctx, &docs);
    if pending.is_empty() {
        eprintln!("no slice-anchored units to embed (are there slices with descriptions?)");
        return Ok(1);
    }

    let embedder = OpenRouterEmbedder::from_env(model, dims)?;
    let texts: Vec<String> = pending.iter().map(|u| u.text.clone()).collect();
    let vectors = embedder.embed(&texts)?;

    let units: Vec<IndexUnit> = pending
        .into_iter()
        .zip(vectors)
        .map(|(u, vector)| IndexUnit {
            slice_id: u.slice_id,
            kind: u.kind.to_owned(),
            anchor: u.anchor,
            text: u.text,
            slice_fp: u.slice_fp,
            vector,
        })
        .collect();

    let mut slice_ids: Vec<&str> = units.iter().map(|u| u.slice_id.as_str()).collect();
    slice_ids.sort_unstable();
    slice_ids.dedup();
    let slice_count = slice_ids.len();

    let index = SemanticIndex {
        provider: "openrouter".to_owned(),
        model: embedder.model_id().to_owned(),
        dims,
        built_at: ctx.head_sha(),
        units,
    };

    let path = ctx.slices_dir().join(INDEX_FILE);
    let json = serde_json::to_string(&index)?;
    std::fs::write(&path, json).map_err(|source| Error::Write {
        path: path.clone(),
        source,
    })?;
    println!(
        "semantic index: {} units across {} slices ({}, {} dims) -> {}",
        index.units.len(),
        slice_count,
        index.model,
        index.dims,
        ctx.rel(&path)
    );
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn doc(slice_id: &str, description: &str, abstractions: &[&str]) -> SliceDoc {
        SliceDoc {
            slice_id: slice_id.to_owned(),
            doc_path: PathBuf::from(format!("slices/{slice_id}.md")),
            description: description.to_owned(),
            loc: None,
            files: vec![],
            abstractions: abstractions.iter().map(|s| (*s).to_owned()).collect(),
            dependencies: vec![],
            exclusions: vec![],
            body: String::new(),
        }
    }

    #[test]
    fn unit_texts_emits_description_then_nonempty_abstractions() {
        let d = doc(
            "auth",
            "Authentication and sessions",
            &["verify_token — JWT", "  ", "rotate — refresh"],
        );
        // description first, then each non-blank abstraction in order; the blank abstraction is dropped.
        assert_eq!(
            unit_texts(&d),
            vec![
                ("description", "Authentication and sessions".to_owned()),
                ("abstraction", "verify_token — JWT".to_owned()),
                ("abstraction", "rotate — refresh".to_owned()),
            ]
        );
    }

    #[test]
    fn unit_texts_skips_a_blank_description() {
        let d = doc("empty", "   ", &["only_abs — x"]);
        assert_eq!(
            unit_texts(&d),
            vec![("abstraction", "only_abs — x".to_owned())]
        );
    }

    #[test]
    fn index_round_trips_through_json() {
        let index = SemanticIndex {
            provider: "openrouter".to_owned(),
            model: "openai/text-embedding-3-small".to_owned(),
            dims: 2,
            built_at: "abc123".to_owned(),
            units: vec![IndexUnit {
                slice_id: "auth".to_owned(),
                kind: "description".to_owned(),
                anchor: Some("src/auth.py:10".to_owned()),
                text: "Authentication".to_owned(),
                slice_fp: "deadbeef".to_owned(),
                vector: vec![0.5, -0.25],
            }],
        };
        let json = serde_json::to_string(&index).unwrap();
        let back: SemanticIndex = serde_json::from_str(&json).unwrap();
        assert_eq!(back.dims, 2);
        assert_eq!(back.units.len(), 1);
        assert_eq!(back.units[0].slice_id, "auth");
        assert_eq!(back.units[0].vector, vec![0.5, -0.25]);
        assert_eq!(back.units[0].anchor.as_deref(), Some("src/auth.py:10"));
    }
}
