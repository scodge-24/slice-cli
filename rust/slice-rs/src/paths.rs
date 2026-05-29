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
    if full.exists() || !has_glob_meta(raw) {
        vec![normalized]
    } else {
        Vec::new()
    }
}

#[must_use]
pub fn repo_join(ctx: &Context, rel: &str) -> PathBuf {
    ctx.repo_root().join(rel)
}

fn has_glob_meta(raw: &str) -> bool {
    raw.bytes().any(|b| matches!(b, b'*' | b'?' | b'['))
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
    use super::matches_path;

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
}
