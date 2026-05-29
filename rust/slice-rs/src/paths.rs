use std::path::{Path, PathBuf};

use crate::context::Context;

#[must_use]
pub fn normalize_repo_path(raw: &str, ctx: &Context) -> String {
    let path = Path::new(raw);
    if path.is_absolute() {
        return path
            .canonicalize()
            .map_or_else(|_| raw.to_owned(), |resolved| ctx.rel(&resolved));
    }
    raw.trim_start_matches("./").to_owned()
}

#[must_use]
pub fn matches_path(path: &str, pattern: &str) -> bool {
    path == pattern || glob_match(pattern.as_bytes(), path.as_bytes())
}

#[must_use]
pub fn expand_literal_or_existing(raw: &str, ctx: &Context) -> Vec<String> {
    let normalized = normalize_repo_path(raw, ctx);
    let full = ctx.repo_root().join(&normalized);
    if !has_glob_meta(raw) {
        return vec![normalized];
    }

    let mut matches = Vec::new();
    collect_matching_files(ctx.repo_root(), ctx.repo_root(), raw, &mut matches);
    matches.sort();
    if matches.is_empty() && full.is_file() {
        matches.push(normalized);
    }
    matches
}

#[must_use]
pub fn repo_join(ctx: &Context, rel: &str) -> PathBuf {
    ctx.repo_root().join(rel)
}

fn has_glob_meta(raw: &str) -> bool {
    raw.bytes().any(|b| matches!(b, b'*' | b'?' | b'['))
}

fn collect_matching_files(root: &Path, dir: &Path, pattern: &str, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_matching_files(root, &path, pattern, out);
        } else if path.is_file() {
            let rel = path
                .strip_prefix(root)
                .unwrap_or(path.as_path())
                .to_string_lossy()
                .into_owned();
            if matches_path(&rel, pattern) {
                out.push(rel);
            }
        }
    }
}

fn glob_match(pattern: &[u8], text: &[u8]) -> bool {
    let (mut p, mut t) = (0, 0);
    let mut star: Option<usize> = None;
    let mut star_text = 0;

    while t < text.len() {
        if p < pattern.len() && (pattern[p] == b'?' || pattern[p] == text[t]) {
            p += 1;
            t += 1;
        } else if p < pattern.len() && pattern[p] == b'*' {
            star = Some(p);
            p += 1;
            star_text = t;
        } else if let Some(star_pos) = star {
            p = star_pos + 1;
            star_text += 1;
            t = star_text;
        } else {
            return false;
        }
    }

    while p < pattern.len() && pattern[p] == b'*' {
        p += 1;
    }
    p == pattern.len()
}

#[cfg(test)]
mod tests {
    use super::{expand_literal_or_existing, matches_path};
    use crate::context::Context;

    #[test]
    fn exact_path_matches() {
        assert!(matches_path(
            "src/auth/middleware.py",
            "src/auth/middleware.py"
        ));
    }

    #[test]
    fn simple_star_glob_matches() {
        assert!(matches_path("src/auth/middleware.py", "src/auth/*.py"));
        assert!(!matches_path("src/models/user.py", "src/auth/*.py"));
    }

    #[test]
    fn literal_glob_metacharacter_file_survives_expansion() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        std::fs::create_dir_all(root.join(".git")).unwrap();
        std::fs::create_dir_all(root.join("app/[id]")).unwrap();
        std::fs::write(root.join("app/[id]/page.tsx"), "").unwrap();
        let ctx = Context::new(Some(root.to_path_buf()), None).unwrap();

        assert_eq!(
            expand_literal_or_existing("app/[id]/page.tsx", &ctx),
            vec!["app/[id]/page.tsx"]
        );
    }

    #[test]
    fn simple_globs_expand_to_existing_files() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        std::fs::create_dir_all(root.join(".git")).unwrap();
        std::fs::create_dir_all(root.join("src/auth")).unwrap();
        std::fs::write(root.join("src/auth/middleware.py"), "").unwrap();
        std::fs::write(root.join("src/auth/sessions.py"), "").unwrap();
        let ctx = Context::new(Some(root.to_path_buf()), None).unwrap();

        assert_eq!(
            expand_literal_or_existing("src/auth/*.py", &ctx),
            vec!["src/auth/middleware.py", "src/auth/sessions.py"]
        );
    }
}
