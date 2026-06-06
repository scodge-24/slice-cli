//! Heuristic source-symbol span detection — hand-rolled, no parser dependency.
//!
//! Used by `slice grep --symbols` (annotate a hit with its enclosing symbol) and by
//! `slice show`/`context` (annotate an abstraction with its definition site). Both are high-trust
//! surfaces, so the governing rule is **reject-on-ambiguity**: when the structure is not
//! unmistakable — decorators, nested functions, multi-line signatures, tab indentation, braces
//! inside strings/comments — return `None` rather than risk emitting a *wrong* span. A wrong span
//! is worse than no span here. A future AST backend can replace this behind the same two functions.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbol {
    pub name: String,
    /// 1-based inclusive line range of the definition.
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    Python,
    Brace,
}

/// Map a path to a supported language family by extension (pure — no IO). Unsupported extensions
/// return `None`, which makes the callers skip annotation entirely.
#[must_use]
pub fn lang_for_path(path: &str) -> Option<Lang> {
    let ext = path.rsplit('.').next().unwrap_or("");
    match ext {
        "py" | "pyi" => Some(Lang::Python),
        "rs" | "js" | "jsx" | "ts" | "tsx" | "go" | "c" | "cc" | "cpp" | "cxx" | "h" | "hpp"
        | "java" => Some(Lang::Brace),
        _ => None,
    }
}

/// The smallest definition whose body encloses 1-based `line`, or `None` when ambiguous/unfound.
#[must_use]
pub fn enclosing_span(content: &str, line: usize, lang: Lang) -> Option<Symbol> {
    let lines: Vec<&str> = content.lines().collect();
    if line == 0 || line > lines.len() {
        return None;
    }
    match lang {
        Lang::Python => py_enclosing(&lines, line - 1),
        Lang::Brace => brace_enclosing(&lines, line - 1),
    }
}

/// The 1-based inclusive line range of the unique definition of `name`, or `None` when the name is
/// not defined exactly once or the definition is ambiguous.
#[must_use]
pub fn definition_span(content: &str, name: &str, lang: Lang) -> Option<(usize, usize)> {
    let lines: Vec<&str> = content.lines().collect();
    let mut hit = None;
    for (i, raw) in lines.iter().enumerate() {
        let defined = match lang {
            Lang::Python => py_def_name(raw).map(|(_, n)| n),
            Lang::Brace => brace_def_name(raw),
        };
        if defined.as_deref() == Some(name) {
            if hit.is_some() {
                return None; // defined more than once → ambiguous
            }
            hit = Some(i);
        }
    }
    let d = hit?;
    let sym = match lang {
        Lang::Python => py_span_from_def(&lines, d)?,
        Lang::Brace => brace_span_from_def(&lines, d)?,
    };
    Some((sym.start, sym.end))
}

/// Enumerate every definition in `content` the heuristic can confidently span, in line order, plus
/// the count of definition lines it had to **skip** (decorators, multi-line signatures, nested
/// functions, tab indentation — the same reject-on-ambiguity cases the point queries refuse).
///
/// The skip count is not incidental: it is the **declared-coverage** signal. An outline that
/// silently omits a decorated or multi-line def reads as complete when it isn't — worse than no
/// outline. Callers MUST surface `(found.len(), found.len() + skipped)` so the consumer knows how
/// much of the file went unrepresented. When declared coverage is too low to trust on real repos,
/// that is itself the evidence that would gate an AST backend.
#[must_use]
pub fn enumerate_symbols(content: &str, lang: Lang) -> (Vec<Symbol>, usize) {
    let lines: Vec<&str> = content.lines().collect();
    let mut found = Vec::new();
    let mut skipped = 0usize;
    for (i, raw) in lines.iter().enumerate() {
        let is_def = match lang {
            Lang::Python => py_def_name(raw).is_some(),
            Lang::Brace => brace_def_name(raw).is_some(),
        };
        if !is_def {
            continue;
        }
        let span = match lang {
            Lang::Python => py_span_from_def(&lines, i),
            Lang::Brace => brace_span_from_def(&lines, i),
        };
        match span {
            Some(sym) => found.push(sym),
            None => skipped += 1,
        }
    }
    (found, skipped)
}

// --- Python (indentation-based) -------------------------------------------------------------

enum Lead {
    Blank,
    Tab,
    Spaces(usize),
}

fn py_lead(s: &str) -> Lead {
    if s.trim().is_empty() {
        return Lead::Blank;
    }
    let lead = &s[..s.len() - s.trim_start().len()];
    if lead.contains('\t') {
        return Lead::Tab;
    }
    Lead::Spaces(lead.len())
}

/// `(kind, name)` for a `def`/`class` line (kind is `"def"` or `"class"`), else `None`.
fn py_def_name(line: &str) -> Option<(&'static str, String)> {
    let t = line.trim_start();
    let t = t.strip_prefix("async ").unwrap_or(t);
    for (kw, kind) in [("def ", "def"), ("class ", "class")] {
        if let Some(rest) = t.strip_prefix(kw) {
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() {
                return Some((kind, name));
            }
        }
    }
    None
}

fn py_enclosing(lines: &[&str], target0: usize) -> Option<Symbol> {
    let d = if py_def_name(lines[target0]).is_some() {
        target0
    } else {
        let Lead::Spaces(ti) = py_lead(lines[target0]) else {
            return None; // blank or tab-indented target → can't anchor
        };
        let mut found = None;
        for i in (0..target0).rev() {
            match py_lead(lines[i]) {
                Lead::Tab => return None,
                Lead::Spaces(li) if li < ti && py_def_name(lines[i]).is_some() => {
                    found = Some(i);
                    break;
                }
                _ => {}
            }
        }
        found?
    };
    py_span_from_def(lines, d)
}

fn py_span_from_def(lines: &[&str], d: usize) -> Option<Symbol> {
    let (_, name) = py_def_name(lines[d])?;
    let Lead::Spaces(def_indent) = py_lead(lines[d]) else {
        return None; // tab-indented def → reject
    };

    // Reject if a decorator sits immediately above the def (the real start is ambiguous).
    let mut j = d;
    while j > 0 {
        let prev = lines[j - 1];
        if prev.trim().is_empty() {
            j -= 1;
            continue;
        }
        if prev.trim_start().starts_with('@') {
            return None;
        }
        break;
    }

    // Reject a nested *function* (immediate structural parent is a `def`). A `class` parent (i.e. a
    // method) is fine.
    for i in (0..d).rev() {
        match py_lead(lines[i]) {
            Lead::Tab => return None,
            Lead::Spaces(li) if li < def_indent => {
                if let Some(("def", _)) = py_def_name(lines[i]) {
                    return None; // nested inside a function, not a class
                }
                break;
            }
            _ => {}
        }
    }

    // Reject a multi-line signature: the def line must have balanced parens and end with ':'.
    let code = lines[d].split('#').next().unwrap_or(lines[d]);
    if code.matches('(').count() != code.matches(')').count() || !code.trim_end().ends_with(':') {
        return None;
    }

    // End = last non-blank line indented deeper than the def; reject on any tab in the body.
    let mut end = d;
    for (i, line) in lines.iter().enumerate().skip(d + 1) {
        match py_lead(line) {
            Lead::Blank => {}
            Lead::Tab => return None,
            Lead::Spaces(li) if li > def_indent => end = i,
            Lead::Spaces(_) => break,
        }
    }
    Some(Symbol {
        name,
        start: d + 1,
        end: end + 1,
    })
}

// --- Brace languages (conservative) ---------------------------------------------------------

/// True when a `{`/`}` on this line might be hidden inside a string, char literal, or comment —
/// which would corrupt brace-depth counting. A line with no brace is never a hazard, so string or
/// comment markers on braceless lines don't suppress annotation. Single quotes count only as
/// `'{'`/`'}'` (a char literal holding a brace); a bare `'` is ignored so Rust lifetimes (`&'a T`)
/// and ordinary char literals don't reject the whole span.
fn brace_may_be_hidden(line: &str) -> bool {
    if !line.contains('{') && !line.contains('}') {
        return false;
    }
    line.contains('"')
        || line.contains('`')
        || line.contains("//")
        || line.contains("/*")
        || line.contains("*/")
        || line.contains('#')
        || line.contains("'{'")
        || line.contains("'}'")
}

const BRACE_KEYWORDS: [&str; 8] = [
    "fn ",
    "func ",
    "function ",
    "def ",
    "class ",
    "struct ",
    "interface ",
    "impl ",
];

fn brace_def_name(line: &str) -> Option<String> {
    let t = line.trim_start();
    for kw in BRACE_KEYWORDS {
        if let Some(rest) = t.strip_prefix(kw) {
            let name: String = rest
                .trim_start()
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() {
                return Some(name);
            }
        }
    }
    None
}

fn brace_span_from_def(lines: &[&str], d: usize) -> Option<Symbol> {
    let name = brace_def_name(lines[d])?;
    // Require the opening brace on the def line; anything else (K&R next-line brace, multi-line
    // signature) is rejected for safety.
    if !lines[d].contains('{') {
        return None;
    }
    let mut depth = 0i32;
    let mut started = false;
    for (i, line) in lines.iter().enumerate().skip(d) {
        if brace_may_be_hidden(line) {
            return None; // a brace could be inside a string/char/comment → ambiguous
        }
        for ch in line.chars() {
            if ch == '{' {
                depth += 1;
                started = true;
            } else if ch == '}' {
                depth -= 1;
            }
        }
        if started && depth <= 0 {
            return Some(Symbol {
                name,
                start: d + 1,
                end: i + 1,
            });
        }
    }
    None
}

fn brace_enclosing(lines: &[&str], target0: usize) -> Option<Symbol> {
    let target1 = target0 + 1;
    // Scan backward for the nearest preceding definition whose span actually encloses the target.
    // A def that fails to parse, or whose span ends before the target (e.g. a nested function the
    // target sits *after*), is skipped so the search keeps climbing toward the real encloser.
    for d in (0..=target0).rev() {
        if brace_def_name(lines[d]).is_some()
            && let Some(sym) = brace_span_from_def(lines, d)
            && (sym.start..=sym.end).contains(&target1)
        {
            return Some(sym);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const PY: &str = "\
def top(a, b):
    x = a + b
    return x


class C:
    def method(self):
        return 1
";

    #[test]
    fn python_top_level_def() {
        // line 2 (`x = a + b`) is inside `top` (lines 1-3).
        let s = enclosing_span(PY, 2, Lang::Python).unwrap();
        assert_eq!((s.name.as_str(), s.start, s.end), ("top", 1, 3));
        // the def line itself resolves to the same span.
        assert_eq!(enclosing_span(PY, 1, Lang::Python).unwrap().name, "top");
    }

    #[test]
    fn python_method_in_class_is_ok() {
        // `method` is nested in a *class*, which is allowed (it's a method, not a closure).
        let s = enclosing_span(PY, 8, Lang::Python).unwrap();
        assert_eq!((s.name.as_str(), s.start, s.end), ("method", 7, 8));
    }

    #[test]
    fn python_decorator_before_def_rejects() {
        let src = "@require_auth\ndef get_user(request):\n    return 1\n";
        assert_eq!(enclosing_span(src, 3, Lang::Python), None);
    }

    #[test]
    fn python_nested_function_rejects() {
        let src = "def outer():\n    def wrapper():\n        return 1\n    return wrapper\n";
        // line 3 is inside the nested `wrapper` → reject rather than mis-span.
        assert_eq!(enclosing_span(src, 3, Lang::Python), None);
    }

    #[test]
    fn python_multiline_signature_rejects() {
        let src = "def long(\n    a,\n    b,\n):\n    return a\n";
        assert_eq!(enclosing_span(src, 5, Lang::Python), None);
    }

    #[test]
    fn python_tab_indent_rejects() {
        let src = "def t():\n\treturn 1\n";
        assert_eq!(enclosing_span(src, 2, Lang::Python), None);
    }

    #[test]
    fn python_top_level_code_returns_none() {
        let src = "import os\nx = 1\n";
        assert_eq!(enclosing_span(src, 2, Lang::Python), None);
    }

    #[test]
    fn python_definition_span_unique_and_ambiguous() {
        assert_eq!(definition_span(PY, "top", Lang::Python), Some((1, 3)));
        let dup = "def f():\n    return 1\n\ndef f():\n    return 2\n";
        assert_eq!(definition_span(dup, "f", Lang::Python), None);
    }

    #[test]
    fn brace_simple_function() {
        let src = "fn main() {\n    let x = 1;\n    foo();\n}\n";
        let s = enclosing_span(src, 3, Lang::Brace).unwrap();
        assert_eq!((s.name.as_str(), s.start, s.end), ("main", 1, 4));
    }

    #[test]
    fn brace_in_string_rejects() {
        // a `}` inside a string literal would corrupt depth counting → reject the whole thing.
        let src = "fn f() {\n    let s = \"}\";\n    g();\n}\n";
        assert_eq!(enclosing_span(src, 3, Lang::Brace), None);
    }

    #[test]
    fn brace_climbs_to_outer_after_nested_fn() {
        // line 5 (`let x = 1;`) sits in `outer` (1-6), *after* the nested `inner` (2-4) — the
        // search must climb past inner instead of giving up (regression: early `return None`).
        let src = "fn outer() {\n    fn inner() {\n        work();\n    }\n    let x = 1;\n}\n";
        let s = enclosing_span(src, 5, Lang::Brace).unwrap();
        assert_eq!((s.name.as_str(), s.start, s.end), ("outer", 1, 6));
        // a line inside the nested fn still resolves to the nested fn.
        assert_eq!(enclosing_span(src, 3, Lang::Brace).unwrap().name, "inner");
    }

    #[test]
    fn brace_rust_lifetime_on_def_line_is_ok() {
        // a `'a` lifetime on the def line must not be mistaken for a brace-hiding char literal.
        let src = "fn parse<'a>(input: &'a str) {\n    let x = 1;\n    work(x);\n}\n";
        let s = enclosing_span(src, 2, Lang::Brace).unwrap();
        assert_eq!((s.name.as_str(), s.start, s.end), ("parse", 1, 4));
    }

    #[test]
    fn brace_braceless_string_line_does_not_suppress() {
        // a string on a *braceless* body line can't corrupt depth counting, so it must not reject
        // the whole span.
        let src = "fn greet() {\n    let msg = \"hello world\";\n    print(msg);\n}\n";
        let s = enclosing_span(src, 3, Lang::Brace).unwrap();
        assert_eq!((s.name.as_str(), s.start, s.end), ("greet", 1, 4));
    }

    #[test]
    fn lang_dispatch() {
        assert_eq!(lang_for_path("a/b/c.py"), Some(Lang::Python));
        assert_eq!(lang_for_path("src/main.rs"), Some(Lang::Brace));
        assert_eq!(lang_for_path("README.md"), None);
    }

    #[test]
    fn enumerate_python_finds_top_level_and_methods() {
        let (syms, skipped) = enumerate_symbols(PY, Lang::Python);
        let got: Vec<(&str, usize, usize)> = syms
            .iter()
            .map(|s| (s.name.as_str(), s.start, s.end))
            .collect();
        // top (1-3), class C (6-8), method (7-8) — all confidently spanned, in line order.
        assert_eq!(got, vec![("top", 1, 3), ("C", 6, 8), ("method", 7, 8)]);
        assert_eq!(skipped, 0);
    }

    #[test]
    fn enumerate_counts_unspannable_defs_as_skipped() {
        // A decorated def cannot be spanned (the real start is ambiguous), so it counts toward the
        // declared-coverage gap rather than silently vanishing: found={clean}, skipped={decorated}.
        let src = "@deco\ndef decorated():\n    return 1\n\ndef clean():\n    return 2\n";
        let (syms, skipped) = enumerate_symbols(src, Lang::Python);
        assert_eq!(
            syms.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
            vec!["clean"]
        );
        assert_eq!(skipped, 1); // declared coverage = 1/2
    }

    #[test]
    fn enumerate_brace_orders_and_skips_kr_brace() {
        // `spanned` (brace on the def line) is found; `kr` (K&R next-line brace) is unspannable.
        let src = "fn spanned() {\n    work();\n}\n\nfn kr()\n{\n    work();\n}\n";
        let (syms, skipped) = enumerate_symbols(src, Lang::Brace);
        assert_eq!(
            syms.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
            vec!["spanned"]
        );
        assert_eq!(skipped, 1);
    }

    #[test]
    fn enumerate_empty_is_empty() {
        let (syms, skipped) = enumerate_symbols("import os\nx = 1\n", Lang::Python);
        assert!(syms.is_empty());
        assert_eq!(skipped, 0);
    }
}
