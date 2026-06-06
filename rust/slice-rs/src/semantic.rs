//! Stage 4 — semantic / hybrid retrieval lens. Compiled only under the `semantic` feature.
//!
//! Embeds slice-ANCHORED units into a staleness-tracked vector index (slice-owned state under
//! `slices/`), and `find --semantic` queries it: cosine generates candidates, deterministic topology
//! owns the rank, and a drifted owning slice flags the hit stale. Three unit surfaces ([`UnitMode`])
//! coexist for A/B comparison: `cards` (description + abstractions — the default thin NL surface),
//! `code` (one symbol-body chunk per definition), and `code-sliceaug` (the same chunks prepended with
//! owning-slice context — contextual-retrieval augmentation). Each mode writes its own index file.
//!
//! Plan §7 guardrails: every unit stays slice-anchored (`slice_id` + `file:line` for code; never an
//! anonymous chunk); the vector score is a CANDIDATE GENERATOR only — topology + freshness rank; the
//! provider is a trait (`OpenRouter` first, a local/offline backend droppable in later); each unit
//! stores its owning slice's content fingerprint so a stale hit is flagged, not silently returned.

use std::cmp::Ordering;

use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

use crate::color::Styles;
use crate::commands::{content_fingerprint, emit_json};
use crate::context::Context;
use crate::models::SliceDoc;
use crate::paths::expand_literal_or_existing;
use crate::slices::load_slice_docs;
use crate::symbols;
use crate::{Error, Result};

const DEFAULT_MODEL: &str = "google/gemini-embedding-2";
/// Matryoshka-trained models (gemini-embedding, text-embedding-3-*) support dimension reduction; 512
/// keeps the on-disk index lean with minimal quality loss versus the native (3072 for gemini).
const DEFAULT_DIMS: usize = 512;
const OPENROUTER_URL: &str = "https://openrouter.ai/api/v1/embeddings";
/// Per-chunk embedding-input cap (chars). Keeps a long symbol body under the model's per-input token
/// limit; the `file:line` anchor is preserved regardless of truncation.
const MAX_CHUNK_CHARS: usize = 6000;

/// Which slice-anchored units get embedded. Selects both what is indexed and the on-disk filename, so
/// the card and code indexes coexist for A/B comparison.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum UnitMode {
    /// Card description + abstractions (the default; thin NL summary surface).
    Cards,
    /// Source-code symbol-body chunks (each anchored to its owning slice + `file:line`).
    Code,
    /// Code chunks prepended with their owning-slice context (`slice_id` + description + the matching
    /// abstraction + runtime-flow lines) — contextual-retrieval augmentation from the curated card.
    #[value(name = "code-sliceaug")]
    CodeSliceAug,
}

impl UnitMode {
    fn label(self) -> &'static str {
        match self {
            Self::Cards => "cards",
            Self::Code => "code",
            Self::CodeSliceAug => "code-sliceaug",
        }
    }

    /// On-disk index filename for this mode (modes coexist so the ruler can compare them).
    fn index_file(self) -> &'static str {
        match self {
            Self::Cards => "SEMANTIC.json",
            Self::Code => "SEMANTIC-code.json",
            Self::CodeSliceAug => "SEMANTIC-code-sliceaug.json",
        }
    }
}

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
        // Chunk so a large slice corpus never builds one oversized request. 96 stays under
        // gemini-embedding's 100-inputs-per-batch cap (OpenAI models allow far more).
        for chunk in texts.chunks(96) {
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
    /// `cards` | `code` | `code-sliceaug` — which unit surface was embedded.
    #[serde(default)]
    pub unit_mode: String,
    /// The retrieval task-type sent to the model, or `null` when unavailable. `OpenRouter`'s Gemini
    /// passthrough ignores `task_type` (verified: DOCUMENT == QUERY == none), so it is recorded as
    /// `null` rather than silently implied — all arms are symmetric embeddings.
    #[serde(default)]
    pub task_type: Option<String>,
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

/// The units to embed for `mode`: card description + abstractions (`Cards`), or one symbol-body chunk
/// per definition (`Code`/`CodeSliceAug`). Every unit stays slice-anchored.
fn extract_units(ctx: &Context, docs: &[SliceDoc], mode: UnitMode) -> Vec<PendingUnit> {
    match mode {
        UnitMode::Cards => card_units(ctx, docs),
        UnitMode::Code | UnitMode::CodeSliceAug => code_units(ctx, docs, mode),
    }
}

/// Attach each slice's fingerprint to its [`unit_texts`], producing the card units to embed.
fn card_units(ctx: &Context, docs: &[SliceDoc]) -> Vec<PendingUnit> {
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

/// The repo-relative source files a slice covers (literal or glob-expanded), deduped.
fn expand_files(ctx: &Context, doc: &SliceDoc) -> Vec<String> {
    let mut paths = Vec::new();
    for raw in &doc.files {
        paths.extend(expand_literal_or_existing(raw, ctx));
    }
    paths.sort();
    paths.dedup();
    paths
}

/// One embedded unit per spannable symbol in the slice's files. `Code` embeds the symbol body alone;
/// `CodeSliceAug` prepends the owning-slice context (the augmentation under test). Chunking is
/// IDENTICAL across the two modes — only the embedded input text differs — so the A/B isolates the
/// augmentation. Each unit carries `slice_id` + `file:line` (never anonymous; §7 guardrail).
fn code_units(ctx: &Context, docs: &[SliceDoc], mode: UnitMode) -> Vec<PendingUnit> {
    let mut units = Vec::new();
    for doc in docs {
        let fp = slice_fingerprint(ctx, doc);
        let flows = runtime_flow_lines(&doc.body);
        for rel in expand_files(ctx, doc) {
            let Some(lang) = symbols::lang_for_path(&rel) else {
                continue;
            };
            let Some(content) = read_repo_file(ctx, &rel) else {
                continue;
            };
            let lines: Vec<&str> = content.lines().collect();
            let (syms, _) = symbols::enumerate_symbols(&content, lang);
            for sym in syms {
                let end = sym.end.min(lines.len()).max(sym.start);
                let body = lines[sym.start - 1..end].join("\n");
                let text = match mode {
                    UnitMode::CodeSliceAug => {
                        let head = slice_context(doc, &sym.name, &flows);
                        truncate_chars(&format!("{head}\n\n{body}"), MAX_CHUNK_CHARS)
                    }
                    _ => truncate_chars(&body, MAX_CHUNK_CHARS),
                };
                units.push(PendingUnit {
                    slice_id: doc.slice_id.clone(),
                    kind: "code",
                    anchor: Some(format!("{rel}:{}", sym.start)),
                    text,
                    slice_fp: fp.clone(),
                });
            }
        }
    }
    units
}

fn read_repo_file(ctx: &Context, rel: &str) -> Option<String> {
    std::fs::read_to_string(ctx.repo_root().join(rel)).ok()
}

/// The owning-slice context prepended to a chunk in `CodeSliceAug`: the slice id + description, the
/// abstraction line naming this symbol (if any), and runtime-flow lines that mention it.
fn slice_context(doc: &SliceDoc, sym_name: &str, flows: &[String]) -> String {
    let mut parts = vec![format!("slice {} — {}", doc.slice_id, doc.description)];
    for abs in &doc.abstractions {
        if abs
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .any(|w| w == sym_name)
        {
            parts.push(abs.clone());
        }
    }
    for flow in flows {
        if flow.contains(sym_name) {
            parts.push(flow.clone());
        }
    }
    parts.join("\n")
}

/// Non-blank lines under the card's `## Runtime Flows` section (until the next `## ` heading).
fn runtime_flow_lines(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut in_flows = false;
    for line in body.lines() {
        if let Some(heading) = line.strip_prefix("## ") {
            in_flows = heading.trim().eq_ignore_ascii_case("Runtime Flows");
            continue;
        }
        if in_flows && !line.trim().is_empty() {
            out.push(line.trim().to_owned());
        }
    }
    out
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_owned();
    }
    s.chars().take(max).collect()
}

/// Build and persist the semantic index for `mode`. Network-bound (one embedding call per ≤96-unit
/// chunk). Each mode writes its own file so card and code indexes coexist for comparison.
pub fn build_index(
    ctx: &Context,
    model: Option<String>,
    dimensions: Option<usize>,
    mode: UnitMode,
) -> Result<i32> {
    let model = model.unwrap_or_else(|| DEFAULT_MODEL.to_owned());
    let dims = dimensions.unwrap_or(DEFAULT_DIMS);
    let docs = load_slice_docs(ctx)?;
    let pending = extract_units(ctx, &docs, mode);
    if pending.is_empty() {
        eprintln!(
            "no {} units to embed (are there slices with source files?)",
            mode.label()
        );
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
        unit_mode: mode.label().to_owned(),
        // OpenRouter's Gemini passthrough ignores task_type (verified), so all arms are symmetric.
        task_type: None,
        built_at: ctx.head_sha(),
        units,
    };

    let path = ctx.slices_dir().join(mode.index_file());
    let json = serde_json::to_string(&index)?;
    std::fs::write(&path, json).map_err(|source| Error::Write {
        path: path.clone(),
        source,
    })?;
    println!(
        "semantic index [{}]: {} units across {} slices ({}, {} dims) -> {}",
        index.unit_mode,
        index.units.len(),
        slice_count,
        index.model,
        index.dims,
        ctx.rel(&path)
    );
    Ok(0)
}

// --- query (find --semantic) ----------------------------------------------------------------

/// Load the persisted index for `mode`, or `None` if it hasn't been built yet.
fn load_index(ctx: &Context, mode: UnitMode) -> Result<Option<SemanticIndex>> {
    let path = ctx.slices_dir().join(mode.index_file());
    match std::fs::read_to_string(&path) {
        Ok(raw) => {
            let index: SemanticIndex = serde_json::from_str(&raw).map_err(|e| {
                Error::InvalidInput(format!("invalid semantic index {}: {e}", ctx.rel(&path)))
            })?;
            Ok(Some(index))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(Error::Read { path, source }),
    }
}

/// The file portion of a code unit's `file:line` anchor, or the `slice_id` as a fallback.
fn unit_file(u: &IndexUnit) -> String {
    u.anchor
        .as_deref()
        .and_then(|a| a.rsplit_once(':'))
        .map_or_else(|| u.slice_id.clone(), |(f, _)| f.to_owned())
}

/// Embed the query with the SAME model/dims the index was built with — vectors are only comparable
/// within one model.
fn embed_query(index: &SemanticIndex, needle: &str) -> Result<Vec<f32>> {
    let embedder = OpenRouterEmbedder::from_env(index.model.clone(), index.dims)?;
    embedder
        .embed(std::slice::from_ref(&needle.to_owned()))?
        .pop()
        .ok_or_else(|| Error::Embedding("no embedding returned for query".to_owned()))
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let (mut dot, mut na, mut nb) = (0.0f32, 0.0f32, 0.0f32);
    for (x, y) in a.iter().zip(b) {
        dot += x * y;
        na += x * x;
        nb += y * y;
    }
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    dot / (na.sqrt() * nb.sqrt())
}

/// Deterministic rank comparator (plan §7: the vector score only *generates* candidates; freshness
/// and topology *own* the ranking). Tuple = `(fresh, breadth, reverse_dep, cosine, slice_id)`. Order:
/// fresh before stale, then more matched units, then more depended-upon, then cosine as the final
/// tiebreak only, then `slice_id` for stability. Cosine never outranks a deterministic signal.
fn rank_cmp(a: (bool, usize, usize, f32, &str), b: (bool, usize, usize, f32, &str)) -> Ordering {
    b.0.cmp(&a.0)
        .then(b.1.cmp(&a.1))
        .then(b.2.cmp(&a.2))
        .then(b.3.partial_cmp(&a.3).unwrap_or(Ordering::Equal))
        .then(a.4.cmp(b.4))
}

/// A query candidate aggregated to its owning slice.
struct Agg<'a> {
    rep: &'a IndexUnit,
    score: f32,
    breadth: usize,
}

#[derive(Serialize)]
struct Hit<'a> {
    slice_id: &'a str,
    /// The actionable navigation target: the owning slice's files (cards mode) or the chunk's single
    /// `file:line`-bearing file (code modes).
    files: Vec<String>,
    /// `fresh` | `stale` (owning slice drifted since build) | `missing` (slice deleted).
    freshness: &'static str,
    /// Why this hit surfaced: the matched unit's kind + text + cosine score (§7: every hit has a reason).
    kind: &'a str,
    text: &'a str,
    score: f32,
}

/// A `Hit` with its deterministic sort keys captured, so re-ranking never re-derives them.
struct Ranked<'a> {
    hit: Hit<'a>,
    fresh: bool,
    breadth: usize,
    revdep: usize,
}

const CANDIDATES: usize = 24;
const SHOW: usize = 10;

/// `find --semantic`: embed the query, generate candidates by cosine, re-rank deterministically, and
/// emit anchored hits with freshness + a reason. Network-bound (embeds the query once). Candidates
/// aggregate to slices (cards mode) or to files (code modes), matching each surface's granularity.
pub fn query(
    ctx: &Context,
    needle: &str,
    json: bool,
    styles: &Styles,
    mode: UnitMode,
) -> Result<i32> {
    let Some(index) = load_index(ctx, mode)? else {
        eprintln!(
            "no {} semantic index; run `slice semantic-index --units {}` first",
            mode.label(),
            mode.label()
        );
        return Ok(1);
    };
    if index.units.is_empty() {
        eprintln!("semantic index is empty; rebuild with `slice semantic-index`");
        return Ok(1);
    }
    let code = index.unit_mode != UnitMode::Cards.label();

    let qvec = embed_query(&index, needle)?;

    let docs = load_slice_docs(ctx)?;
    let by_id: FxHashMap<&str, &SliceDoc> = docs.iter().map(|d| (d.slice_id.as_str(), d)).collect();
    let mut revdep: FxHashMap<&str, usize> = FxHashMap::default();
    for d in &docs {
        for dep in &d.dependencies {
            *revdep.entry(dep.as_str()).or_default() += 1;
        }
    }

    // Candidate generation: cosine over every unit, keep the top-K units.
    let mut scored: Vec<(usize, f32)> = index
        .units
        .iter()
        .enumerate()
        .map(|(i, u)| (i, cosine(&qvec, &u.vector)))
        .collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));
    scored.truncate(CANDIDATES);

    // Aggregate candidates by slice (cards) or by file (code): best unit + breadth (matched-unit
    // count). The owning slice (rep.slice_id) drives freshness/reverse-dep either way.
    let mut by_key: FxHashMap<String, Agg> = FxHashMap::default();
    for (i, score) in &scored {
        let u = &index.units[*i];
        let key = if code {
            unit_file(u)
        } else {
            u.slice_id.clone()
        };
        let entry = by_key.entry(key).or_insert(Agg {
            rep: u,
            score: *score,
            breadth: 0,
        });
        entry.breadth += 1;
        if *score > entry.score {
            entry.score = *score;
            entry.rep = u;
        }
    }

    // Build each hit with its deterministic sort keys captured alongside, then re-rank.
    let mut ranked: Vec<Ranked> = by_key
        .iter()
        .map(|(key, agg)| {
            let owner = agg.rep.slice_id.as_str();
            let freshness = match by_id.get(owner) {
                None => "missing",
                Some(d) if slice_fingerprint(ctx, d) == agg.rep.slice_fp => "fresh",
                Some(_) => "stale",
            };
            // Code modes report the chunk's single file; cards report the owning slice's file list.
            let files = if code {
                vec![key.clone()]
            } else {
                by_id
                    .get(owner)
                    .map(|d| d.files.clone())
                    .unwrap_or_default()
            };
            Ranked {
                fresh: freshness == "fresh",
                breadth: agg.breadth,
                revdep: revdep.get(owner).copied().unwrap_or(0),
                hit: Hit {
                    slice_id: owner,
                    files,
                    freshness,
                    kind: &agg.rep.kind,
                    text: &agg.rep.text,
                    score: agg.score,
                },
            }
        })
        .collect();
    ranked.sort_by(|a, b| {
        rank_cmp(
            (a.fresh, a.breadth, a.revdep, a.hit.score, a.hit.slice_id),
            (b.fresh, b.breadth, b.revdep, b.hit.score, b.hit.slice_id),
        )
    });
    ranked.truncate(SHOW);
    let hits: Vec<Hit> = ranked.into_iter().map(|r| r.hit).collect();

    if json {
        emit_json(&hits)?;
        return Ok(i32::from(hits.is_empty()));
    }
    if hits.is_empty() {
        eprintln!("no semantic matches for: {needle}");
        return Ok(1);
    }
    print_hits(&hits, styles);
    Ok(0)
}

/// Human output: one line per hit — `slice_id  [freshness]  files  — kind: reason (sim score)`.
fn print_hits(hits: &[Hit], styles: &Styles) {
    let width = hits.iter().map(|h| h.slice_id.len()).max().unwrap_or(0);
    for h in hits {
        let pad = " ".repeat(width.saturating_sub(h.slice_id.len()));
        let tag = styles.paint(styles.label, &format!("[{}]", h.freshness));
        let files = if h.files.is_empty() {
            "(no files)".to_owned()
        } else {
            h.files.join(", ")
        };
        println!(
            "{}{pad}  {tag}  {files}  — {}: {} (sim {:.2})",
            styles.paint(styles.id, h.slice_id),
            h.kind,
            snippet(h.text),
            h.score,
        );
    }
}

/// One-line preview of a unit's text for the human "reason" column.
fn snippet(text: &str) -> String {
    const MAX: usize = 60;
    let one_line = text.replace('\n', " ");
    if one_line.chars().count() <= MAX {
        return one_line;
    }
    let cut: String = one_line.chars().take(MAX).collect();
    format!("{cut}…")
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
            unit_mode: "cards".to_owned(),
            task_type: None,
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

    #[test]
    fn cosine_is_1_for_identical_0_for_orthogonal_and_safe_on_zero() {
        assert!((cosine(&[1.0, 2.0, 3.0], &[1.0, 2.0, 3.0]) - 1.0).abs() < 1e-6);
        assert!(cosine(&[1.0, 0.0], &[0.0, 1.0]).abs() < 1e-6);
        assert!(cosine(&[0.0, 0.0], &[1.0, 1.0]).abs() < f32::EPSILON); // zero vector → 0, not NaN
    }

    /// Sort a set of `(fresh, breadth, revdep, score, id)` rows with `rank_cmp` and return the id order.
    fn order(mut rows: Vec<(bool, usize, usize, f32, &str)>) -> Vec<&str> {
        rows.sort_by(|a, b| rank_cmp(*a, *b));
        rows.into_iter().map(|r| r.4).collect()
    }

    #[test]
    fn rank_freshness_and_topology_own_the_order_cosine_only_breaks_ties() {
        // A stale hit is demoted below a fresh one even with far higher cosine.
        assert_eq!(
            order(vec![
                (false, 9, 9, 0.99, "stale"),
                (true, 1, 0, 0.10, "fresh")
            ]),
            vec!["fresh", "stale"]
        );
        // Among fresh hits: more matched units (breadth) outranks more cosine.
        assert_eq!(
            order(vec![
                (true, 1, 0, 0.99, "narrow"),
                (true, 3, 0, 0.20, "broad")
            ]),
            vec!["broad", "narrow"]
        );
        // Equal fresh+breadth: more depended-upon (reverse-dep) outranks cosine.
        assert_eq!(
            order(vec![(true, 2, 0, 0.99, "leaf"), (true, 2, 5, 0.20, "core")]),
            vec!["core", "leaf"]
        );
        // Only when fresh+breadth+revdep all tie does cosine decide, then slice_id for stability.
        assert_eq!(
            order(vec![(true, 1, 0, 0.30, "b"), (true, 1, 0, 0.80, "a")]),
            vec!["a", "b"]
        );
    }

    #[test]
    fn snippet_flattens_newlines_and_truncates() {
        assert_eq!(snippet("short text"), "short text");
        assert_eq!(snippet("line one\nline two"), "line one line two");
        let long = "x".repeat(80);
        let s = snippet(&long);
        assert!(s.ends_with('…') && s.chars().count() == 61); // 60 chars + ellipsis
    }

    #[test]
    fn runtime_flow_lines_extracts_only_the_flows_section() {
        let body = "lede\n\n## System Behavior\nbehaviour line\n\n## Runtime Flows\nrequest -> verify_token -> handler\nsecond flow\n\n## Verification\nnot a flow\n";
        assert_eq!(
            runtime_flow_lines(body),
            vec![
                "request -> verify_token -> handler".to_owned(),
                "second flow".to_owned()
            ]
        );
    }

    #[test]
    fn slice_context_includes_matching_abstraction_and_flow_only() {
        let d = doc(
            "auth",
            "Authentication and sessions",
            &["verify_token — JWT verification", "rotate — refresh"],
        );
        let flows = vec!["request -> verify_token -> handler".to_owned()];
        let head = slice_context(&d, "verify_token", &flows);
        // header + the verify_token abstraction + the flow mentioning it; the `rotate` abstraction is excluded.
        assert_eq!(
            head,
            "slice auth — Authentication and sessions\nverify_token — JWT verification\nrequest -> verify_token -> handler"
        );
        // a symbol named by no abstraction/flow gets just the header.
        assert_eq!(
            slice_context(&d, "unrelated", &flows),
            "slice auth — Authentication and sessions"
        );
    }

    #[test]
    fn unit_file_takes_the_path_before_the_line_number() {
        let u = IndexUnit {
            slice_id: "s".to_owned(),
            kind: "code".to_owned(),
            anchor: Some("src/auth/middleware.py:42".to_owned()),
            text: String::new(),
            slice_fp: String::new(),
            vector: vec![],
        };
        assert_eq!(unit_file(&u), "src/auth/middleware.py");
    }

    #[test]
    fn unit_mode_files_are_distinct_per_mode() {
        assert_eq!(UnitMode::Cards.index_file(), "SEMANTIC.json");
        assert_ne!(UnitMode::Code.index_file(), UnitMode::Cards.index_file());
        assert_ne!(
            UnitMode::CodeSliceAug.index_file(),
            UnitMode::Code.index_file()
        );
    }
}
