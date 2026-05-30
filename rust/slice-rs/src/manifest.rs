use std::fs;

use indexmap::IndexMap;
use serde::Deserialize;

use crate::context::Context;
use crate::models::{DocManifest, TrackedDoc};
use crate::{Error, Result};

#[derive(Debug, Deserialize)]
struct RawManifest {
    #[serde(alias = "vault_root")]
    docs_root: Option<String>,
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
            docs_root_raw: None,
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
        docs_root_raw: raw.docs_root,
        docs,
    })
}

pub fn save_doc_manifest(manifest: &DocManifest, ctx: &Context) -> Result<()> {
    let mut content = String::new();
    if let Some(docs_root) = &manifest.docs_root_raw {
        content.push_str("docs_root: ");
        content.push_str(docs_root);
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

#[cfg(test)]
mod tests {
    use super::{load_doc_manifest, save_doc_manifest};
    use crate::context::Context;

    fn ctx(root: &std::path::Path) -> Context {
        Context::from_parts_for_test(root.to_path_buf(), root.join(".git"), root.join("slices"))
    }

    #[test]
    fn load_returns_empty_without_manifest() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir(temp.path().join("slices")).unwrap();
        let manifest = load_doc_manifest(&ctx(temp.path())).unwrap();
        assert!(manifest.docs.is_empty());
        assert_eq!(manifest.docs_root_raw, None);
    }

    #[test]
    fn load_preserves_fields_and_docs_root() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir(temp.path().join("slices")).unwrap();
        std::fs::write(
            temp.path().join("slices/DOCS.yaml"),
            "docs_root: ../docs\ndocs:\n  guide:\n    path: guide.md\n    slices:\n    - auth\n    verified_at: abc123\n    fingerprint: fp\n    tags:\n    - t1\n    include:\n    - src/*.rs\n    exclude:\n    - src/generated.rs\n",
        )
        .unwrap();
        let manifest = load_doc_manifest(&ctx(temp.path())).unwrap();
        assert_eq!(manifest.docs_root_raw.as_deref(), Some("../docs"));
        assert_eq!(manifest.docs[0].doc_id, "guide");
        assert_eq!(manifest.docs[0].path, "guide.md");
        assert_eq!(manifest.docs[0].slices, vec!["auth"]);
        assert_eq!(manifest.docs[0].verified_at, "abc123");
        assert_eq!(manifest.docs[0].fingerprint, "fp");
        assert_eq!(manifest.docs[0].tags, vec!["t1"]);
        assert_eq!(manifest.docs[0].include, vec!["src/*.rs"]);
        assert_eq!(manifest.docs[0].exclude, vec!["src/generated.rs"]);
    }

    #[test]
    fn load_accepts_legacy_vault_root_alias() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir(temp.path().join("slices")).unwrap();
        std::fs::write(
            temp.path().join("slices/DOCS.yaml"),
            "vault_root: ../docs\ndocs:\n  guide:\n    path: guide.md\n    verified_at: abc123\n",
        )
        .unwrap();
        let manifest = load_doc_manifest(&ctx(temp.path())).unwrap();
        assert_eq!(manifest.docs_root_raw.as_deref(), Some("../docs"));
    }

    #[test]
    fn round_trip_migrates_legacy_key_to_docs_root() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir(temp.path().join("slices")).unwrap();
        let path = temp.path().join("slices/DOCS.yaml");
        // Legacy file keyed on `vault_root:`.
        std::fs::write(
            &path,
            "vault_root: ../docs\ndocs:\n  guide:\n    path: guide.md\n    slices:\n    - auth\n    verified_at: abc123\n",
        )
        .unwrap();
        let manifest = load_doc_manifest(&ctx(temp.path())).unwrap();
        save_doc_manifest(&manifest, &ctx(temp.path())).unwrap();
        let written = std::fs::read_to_string(&path).unwrap();
        assert!(
            written.contains("docs_root: ../docs"),
            "writer must emit docs_root: got {written}"
        );
        assert!(
            !written.contains("vault_root:"),
            "writer must not re-emit the legacy key: got {written}"
        );
    }
}
