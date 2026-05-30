use std::env;
use std::path::{Path, PathBuf};

use crate::{Error, Result};

#[derive(Debug, Clone)]
pub struct Context {
    repo_root: PathBuf,
    git_root: PathBuf,
    slices_dir: PathBuf,
}

impl Context {
    #[cfg(test)]
    #[must_use]
    pub(crate) fn from_parts_for_test(
        repo_root: PathBuf,
        git_root: PathBuf,
        slices_dir: PathBuf,
    ) -> Self {
        Self {
            repo_root,
            git_root,
            slices_dir,
        }
    }

    pub fn new(repo: Option<PathBuf>, slices_dir: Option<PathBuf>) -> Result<Self> {
        let repo_root = match repo {
            Some(path) => path
                .canonicalize()
                .map_err(|source| Error::Read { path, source })?,
            None => discover_repo_root()?,
        };
        let slices_dir = match slices_dir {
            Some(path) => path
                .canonicalize()
                .unwrap_or_else(|_| absolutize(&path).unwrap_or(path)),
            None => repo_root.join("slices"),
        };
        let git_root = discover_ancestor_git_root(&repo_root).unwrap_or_else(|| repo_root.clone());
        Ok(Self {
            repo_root,
            git_root,
            slices_dir,
        })
    }

    #[must_use]
    pub fn repo_root(&self) -> &Path {
        &self.repo_root
    }

    #[must_use]
    pub fn git_relative_path(&self, repo_relative: &str) -> String {
        self.repo_root.strip_prefix(&self.git_root).map_or_else(
            |_| repo_relative.to_owned(),
            |prefix| prefix.join(repo_relative).to_string_lossy().into_owned(),
        )
    }

    #[must_use]
    pub fn slices_dir(&self) -> &Path {
        &self.slices_dir
    }

    #[must_use]
    pub fn docs_manifest_path(&self) -> PathBuf {
        self.slices_dir.join("DOCS.yaml")
    }

    #[must_use]
    pub fn index_path(&self) -> PathBuf {
        self.slices_dir.join("INDEX.md")
    }

    #[must_use]
    pub fn head_sha(&self) -> String {
        let Ok(output) = std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&self.repo_root)
            .output()
        else {
            return "unknown".to_owned();
        };
        if output.status.success() {
            String::from_utf8_lossy(&output.stdout).trim().to_owned()
        } else {
            "unknown".to_owned()
        }
    }

    #[must_use]
    pub fn rel(&self, path: &Path) -> String {
        path.strip_prefix(&self.repo_root).map_or_else(
            |_| path.to_string_lossy().into_owned(),
            |p| p.to_string_lossy().into_owned(),
        )
    }
}

fn discover_repo_root() -> Result<PathBuf> {
    if let Ok(env_root) = env::var("SLICE_REPO") {
        return PathBuf::from(env_root)
            .canonicalize()
            .map_err(|source| Error::Read {
                path: PathBuf::from("SLICE_REPO"),
                source,
            });
    }
    if let Ok(env_root) = env::var("SLICES_REPO_ROOT") {
        return PathBuf::from(env_root)
            .canonicalize()
            .map_err(|source| Error::Read {
                path: PathBuf::from("SLICES_REPO_ROOT"),
                source,
            });
    }

    let mut current = env::current_dir()?;
    loop {
        if current.join(".git").exists() {
            return Ok(current);
        }
        if !current.pop() {
            return Err(Error::NotInRepo);
        }
    }
}

fn discover_ancestor_git_root(start: &Path) -> Option<PathBuf> {
    let mut current = start.to_path_buf();
    loop {
        if current.join(".git").exists() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

fn absolutize(path: &Path) -> std::io::Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(env::current_dir()?.join(path))
    }
}

#[cfg(test)]
mod tests {
    use super::Context;

    #[test]
    fn paths_are_derived_from_context_roots() {
        let repo = std::path::PathBuf::from("/tmp/work/repo");
        let git = std::path::PathBuf::from("/tmp/work");
        let ctx = Context::from_parts_for_test(repo.clone(), git, repo.join("custom-slices"));

        assert_eq!(
            ctx.docs_manifest_path(),
            repo.join("custom-slices/DOCS.yaml")
        );
        assert_eq!(ctx.rel(&repo.join("src/lib.rs")), "src/lib.rs");
        assert_eq!(ctx.git_relative_path("src/lib.rs"), "repo/src/lib.rs");
    }

    #[test]
    fn head_sha_is_unknown_outside_git_repo() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = Context::from_parts_for_test(
            temp.path().to_path_buf(),
            temp.path().to_path_buf(),
            temp.path().join("slices"),
        );
        assert_eq!(ctx.head_sha(), "unknown");
    }
}
