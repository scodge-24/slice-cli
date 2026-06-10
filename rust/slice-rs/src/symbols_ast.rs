//! Tree-sitter AST backend for symbol spans (Stage 3) — compiled only under the `ast` feature.
//!
//! This swaps in *behind* `symbols.rs`'s three span operations (`enclosing_span`,
//! `definition_span`, `enumerate_symbols`) for the languages it supports. The heuristic in
//! `symbols.rs` is reject-on-ambiguity: decorators, multi-line signatures, nested functions, and tab
//! indentation all return `None`. A real parser has none of those blind spots, so where the AST
//! backend is active it is a strict precision upgrade — every definition gets a confident span,
//! including the exact cases the heuristic had to skip.
//!
//! Scope (per the plan's §6 discipline): an internal backend behind the existing interface, **not** a
//! new public surface and **not** a persistent index. Python only for now — the whole benchmark
//! surface is Python and it is where the heuristic's reject cases (decorators above all) are densest.
//! Unsupported languages return `None` from these helpers so the caller keeps the heuristic — the
//! backend can only ever upgrade precision, never regress an already-handled language.

use crate::symbols::{Lang, Symbol};
use std::sync::OnceLock;
use tree_sitter::{Node, Parser, Point, Tree};

/// Runtime escape hatch. Even in an `ast` build, `SLICE_SYMBOLS=heuristic` forces the old path —
/// lets the benchmark A/B both backends from one binary, and is a safe rollback if a parse-based
/// span ever looks wrong in the field. Read once.
#[must_use]
pub fn enabled() -> bool {
    static EN: OnceLock<bool> = OnceLock::new();
    *EN.get_or_init(|| std::env::var("SLICE_SYMBOLS").as_deref() != Ok("heuristic"))
}

/// Languages the AST backend handles. Everything else falls through to the heuristic.
#[must_use]
pub fn supports(lang: Lang) -> bool {
    matches!(lang, Lang::Python)
}

const DEF_KINDS: [&str; 2] = ["function_definition", "class_definition"];

fn parse(content: &str, lang: Lang) -> Option<Tree> {
    let language: tree_sitter::Language = match lang {
        Lang::Python => tree_sitter_python::LANGUAGE.into(),
        Lang::Brace => return None,
    };
    let mut parser = Parser::new();
    parser.set_language(&language).ok()?;
    parser.parse(content, None)
}

/// The `Symbol` for a `function_definition`/`class_definition` node. The span extends up to a
/// wrapping `decorated_definition` so a decorated def reports its decorators as part of the
/// definition (the full unit a reader wants — and exactly what the heuristic could not represent).
fn def_symbol(node: Node, src: &[u8]) -> Option<Symbol> {
    let name = node
        .child_by_field_name("name")?
        .utf8_text(src)
        .ok()?
        .to_string();
    let span_node = match node.parent() {
        Some(p) if p.kind() == "decorated_definition" => p,
        _ => node,
    };
    Some(Symbol {
        name,
        start: span_node.start_position().row + 1,
        end: span_node.end_position().row + 1,
    })
}

fn collect(node: Node, src: &[u8], out: &mut Vec<Symbol>) {
    if DEF_KINDS.contains(&node.kind())
        && let Some(sym) = def_symbol(node, src)
    {
        out.push(sym);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect(child, src, out);
    }
}

/// Every definition in `content`, in line order. The second tuple element mirrors the heuristic's
/// "skipped" count — the AST backend confidently spans everything it finds, so it is always 0. That
/// is the visible Stage-3 win: declared coverage goes from the heuristic's ~84–86% to a full 100%.
#[must_use]
pub fn enumerate_symbols(content: &str, lang: Lang) -> Option<(Vec<Symbol>, usize)> {
    let tree = parse(content, lang)?;
    let mut out = Vec::new();
    collect(tree.root_node(), content.as_bytes(), &mut out);
    out.sort_by_key(|s| (s.start, s.end));
    Some((out, 0))
}

/// The innermost definition whose body encloses 1-based `line`, or `None` if the line sits in no
/// definition. A cursor on a decorator line resolves to the def it decorates.
#[must_use]
pub fn enclosing_span(content: &str, line: usize, lang: Lang) -> Option<Symbol> {
    let tree = parse(content, lang)?;
    let src = content.as_bytes();
    let pt = Point {
        row: line.checked_sub(1)?,
        column: 0,
    };
    let mut node = tree.root_node().descendant_for_point_range(pt, pt)?;
    loop {
        if DEF_KINDS.contains(&node.kind()) {
            return def_symbol(node, src);
        }
        if node.kind() == "decorated_definition" {
            let mut cursor = node.walk();
            if let Some(def) = node
                .children(&mut cursor)
                .find(|c| DEF_KINDS.contains(&c.kind()))
            {
                return def_symbol(def, src);
            }
        }
        node = node.parent()?;
    }
}

/// The line range of the unique definition named `name`, or `None` when it is defined zero or more
/// than once. Preserves the heuristic's reject-on-ambiguity contract so callers' guarantees are
/// unchanged — only the per-definition spans get more accurate.
#[must_use]
pub fn definition_span(content: &str, name: &str, lang: Lang) -> Option<(usize, usize)> {
    let tree = parse(content, lang)?;
    let mut all = Vec::new();
    collect(tree.root_node(), content.as_bytes(), &mut all);
    let mut hits = all.into_iter().filter(|s| s.name == name);
    let first = hits.next()?;
    if hits.next().is_some() {
        return None; // defined more than once → ambiguous
    }
    Some((first.start, first.end))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spans_a_decorated_def_the_heuristic_rejects() {
        // The heuristic returns None on a decorated def (real start ambiguous); the AST backend spans
        // it, decorators included.
        let src = "@require_auth\ndef get_user(request):\n    return 1\n";
        let s = enclosing_span(src, 3, Lang::Python).unwrap();
        assert_eq!((s.name.as_str(), s.start, s.end), ("get_user", 1, 3));
        // a cursor on the decorator line resolves to the same def.
        assert_eq!(
            enclosing_span(src, 1, Lang::Python).unwrap().name,
            "get_user"
        );
    }

    #[test]
    fn spans_a_multiline_signature_the_heuristic_rejects() {
        let src = "def long(\n    a,\n    b,\n):\n    return a\n";
        let s = enclosing_span(src, 5, Lang::Python).unwrap();
        assert_eq!((s.name.as_str(), s.start, s.end), ("long", 1, 5));
    }

    #[test]
    fn resolves_nested_function_to_innermost() {
        // The heuristic rejects a line inside a nested function; the AST backend returns the inner def.
        let src = "def outer():\n    def wrapper():\n        return 1\n    return wrapper\n";
        let inner = enclosing_span(src, 3, Lang::Python).unwrap();
        assert_eq!(
            (inner.name.as_str(), inner.start, inner.end),
            ("wrapper", 2, 3)
        );
        // a line in outer's own body resolves to outer.
        assert_eq!(enclosing_span(src, 4, Lang::Python).unwrap().name, "outer");
    }

    #[test]
    fn enumerate_spans_everything_zero_skipped() {
        // Decorated + multi-line + nested — the heuristic would skip all three; the AST backend spans
        // every definition, so declared coverage is full (skipped == 0).
        let src = "@deco\ndef decorated():\n    return 1\n\ndef outer():\n    def inner():\n        return 2\n";
        let (syms, skipped) = enumerate_symbols(src, Lang::Python).unwrap();
        let names: Vec<&str> = syms.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["decorated", "outer", "inner"]);
        assert_eq!(skipped, 0);
    }

    #[test]
    fn definition_span_unique_and_ambiguous() {
        let src = "@deco\ndef f():\n    return 1\n";
        assert_eq!(definition_span(src, "f", Lang::Python), Some((1, 3)));
        let dup = "def g():\n    return 1\n\ndef g():\n    return 2\n";
        assert_eq!(definition_span(dup, "g", Lang::Python), None);
    }

    #[test]
    fn unsupported_language_returns_none_so_caller_keeps_heuristic() {
        assert!(enclosing_span("fn main() {}\n", 1, Lang::Brace).is_none());
        assert!(enumerate_symbols("fn main() {}\n", Lang::Brace).is_none());
    }
}
