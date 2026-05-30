//! Terminal color for the human-readable output paths.
//!
//! Color is TTY-gated: it only ever decorates the text path, never `--json` (which is
//! a contract consumed by agents). When disabled, [`Styles`] holds default/empty
//! `anstyle::Style` values and the `paint`/`highlight` helpers return text unchanged,
//! so every styled call site degrades to a no-op and the output is byte-for-byte the
//! same as before color existed.

use std::io::IsTerminal;

use anstyle::{AnsiColor, Style};
use clap::ValueEnum;

/// When to emit terminal color. `Auto` honors `isatty` + `NO_COLOR`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
#[value(rename_all = "lower")]
pub enum ColorChoice {
    /// Color when stdout is a terminal and `NO_COLOR` is unset.
    #[default]
    Auto,
    /// Always color, even when piped (used by `slice browse`'s fzf preview).
    Always,
    /// Never color.
    Never,
}

impl ColorChoice {
    /// Whether color should be emitted on stdout for this choice.
    ///
    /// `Always` overrides `NO_COLOR`; `Auto` requires both a tty and an unset/empty
    /// `NO_COLOR` (the [no-color.org](https://no-color.org) convention).
    #[must_use]
    pub fn enabled(self) -> bool {
        match self {
            ColorChoice::Always => true,
            ColorChoice::Never => false,
            ColorChoice::Auto => {
                std::io::stdout().is_terminal()
                    && std::env::var_os("NO_COLOR").is_none_or(|value| value.is_empty())
            }
        }
    }
}

/// A resolved palette. Construct via [`Styles::resolve`].
///
/// Named ANSI colors only (no truecolor) so terminals remap them to the active theme
/// and they stay readable on light/solarized backgrounds.
#[derive(Debug, Clone, Copy)]
pub struct Styles {
    enabled: bool,
    /// Slice ids.
    pub id: Style,
    /// Secondary metadata: `LoC`, `[N docs]`, key labels, paths.
    pub dim: Style,
    /// Stale markers and counts.
    pub stale: Style,
    /// Dependency names.
    pub dep: Style,
    /// `find` match-field labels.
    pub label: Style,
    /// Highlighted search needle.
    pub needle: Style,
}

impl Styles {
    /// Resolve the palette for a [`ColorChoice`] against the current stdout/env.
    #[must_use]
    pub fn resolve(choice: ColorChoice) -> Self {
        if choice.enabled() {
            Self::palette()
        } else {
            Self::plain()
        }
    }

    /// Whether this palette emits color.
    #[must_use]
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    fn plain() -> Self {
        let s = Style::new();
        Self {
            enabled: false,
            id: s,
            dim: s,
            stale: s,
            dep: s,
            label: s,
            needle: s,
        }
    }

    fn palette() -> Self {
        Self {
            enabled: true,
            id: Style::new().fg_color(Some(AnsiColor::Cyan.into())).bold(),
            dim: Style::new().dimmed(),
            stale: Style::new().fg_color(Some(AnsiColor::Red.into())).bold(),
            dep: Style::new().fg_color(Some(AnsiColor::Blue.into())),
            label: Style::new().fg_color(Some(AnsiColor::Yellow.into())),
            needle: Style::new().fg_color(Some(AnsiColor::Yellow.into())).bold(),
        }
    }

    /// Wrap `text` in `style`. Returns `text` unchanged when color is disabled, so
    /// callers can wrap unconditionally without affecting plain output.
    #[must_use]
    pub fn paint(&self, style: Style, text: &str) -> String {
        if self.enabled {
            format!("{}{text}{}", style.render(), style.render_reset())
        } else {
            text.to_owned()
        }
    }

    /// Highlight every (case-insensitive) occurrence of `needle` in `text` with the
    /// [`needle`](Self::needle) style. Only inserts escape codes — the visible
    /// characters are preserved, so `strip_ansi(highlight(t)) == t`. No-op when color
    /// is disabled or `needle` is empty.
    #[must_use]
    pub fn highlight(&self, text: &str, needle: &str) -> String {
        if !self.enabled || needle.is_empty() {
            return text.to_owned();
        }
        let haystack = text.to_lowercase();
        let lower_needle = needle.to_lowercase();
        let mut out = String::with_capacity(text.len());
        let mut cursor = 0;
        while let Some(rel) = haystack[cursor..].find(&lower_needle) {
            let start = cursor + rel;
            let end = start + lower_needle.len();
            out.push_str(&text[cursor..start]);
            out.push_str(&self.paint(self.needle, &text[start..end]));
            cursor = end;
        }
        out.push_str(&text[cursor..]);
        out
    }
}

/// Single-quote a path for safe interpolation into a shell command string (used when
/// building `slice browse`'s fzf `--preview` / `--bind` actions, which fzf runs via
/// `$SHELL -c`). Wraps in single quotes and escapes embedded single quotes as `'\''`.
#[must_use]
pub fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strip_ansi(text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut chars = text.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\u{1b}' {
                // CSI: ESC '[' (params/intermediates) final-byte in @..~.
                if chars.peek() == Some(&'[') {
                    chars.next();
                    for next in chars.by_ref() {
                        if ('@'..='~').contains(&next) {
                            break;
                        }
                    }
                }
            } else {
                out.push(c);
            }
        }
        out
    }

    #[test]
    fn auto_is_off_when_not_a_tty() {
        // Tests run with piped stdout, so Auto must resolve to disabled.
        assert!(!ColorChoice::Auto.enabled());
    }

    #[test]
    fn always_and_never_are_unconditional() {
        assert!(ColorChoice::Always.enabled());
        assert!(!ColorChoice::Never.enabled());
    }

    #[test]
    fn disabled_palette_is_a_no_op() {
        let styles = Styles::resolve(ColorChoice::Never);
        assert_eq!(styles.paint(styles.id, "auth-service"), "auth-service");
        assert_eq!(styles.highlight("auth-service", "auth"), "auth-service");
    }

    #[test]
    fn enabled_paint_round_trips_under_strip() {
        let styles = Styles::resolve(ColorChoice::Always);
        let painted = styles.paint(styles.id, "auth-service");
        assert_ne!(painted, "auth-service");
        assert!(painted.contains('\u{1b}'));
        assert_eq!(strip_ansi(&painted), "auth-service");
    }

    #[test]
    fn highlight_preserves_characters_and_case() {
        let styles = Styles::resolve(ColorChoice::Always);
        let out = styles.highlight("Authentication AUTH auth", "auth");
        assert!(out.contains('\u{1b}'));
        // Stripping the escapes must restore the exact original text (case intact).
        assert_eq!(strip_ansi(&out), "Authentication AUTH auth");
    }

    #[test]
    fn shell_quote_escapes_single_quotes_and_wraps() {
        assert_eq!(shell_quote("/home/me/repo"), "'/home/me/repo'");
        assert_eq!(shell_quote("/has space/x"), "'/has space/x'");
        assert_eq!(shell_quote("a'b"), "'a'\\''b'");
        assert_eq!(shell_quote("p)("), "'p)('");
    }
}
