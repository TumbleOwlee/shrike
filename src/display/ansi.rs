use std::sync::OnceLock;

use regex::Regex;
use terminal_size::{terminal_size, Width};

static ANSI_RE: OnceLock<Regex> = OnceLock::new();

fn ansi_re() -> &'static Regex {
    ANSI_RE.get_or_init(|| Regex::new(r"\x1b\[[0-9;]*[A-Za-z]").unwrap())
}

pub fn strip(s: &str) -> String {
    ansi_re().replace_all(s, "").into_owned()
}

pub fn terminal_width() -> usize {
    terminal_size()
        .map(|(Width(w), _)| w as usize)
        .unwrap_or(80)
        .min(120)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_color() {
        assert_eq!(strip("\x1b[32mhello\x1b[0m"), "hello");
    }

    #[test]
    fn strip_bold() {
        assert_eq!(strip("\x1b[1mfoo\x1b[0m bar"), "foo bar");
    }

    #[test]
    fn no_ansi_unchanged() {
        assert_eq!(strip("plain text"), "plain text");
    }
}
