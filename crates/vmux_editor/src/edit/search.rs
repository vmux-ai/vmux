use std::ops::Range;

/// Translate a vim pattern into the `regex` crate's syntax.
///
/// Vim's default "magic" level makes `.`, `*`, `[`, `^`, and `$` special while requiring a
/// backslash for `+`, `?`, `(`, `)`, `{`, `}`, and `|` — the opposite of `regex`. `\v` (very
/// magic) is already close to `regex`, and `\V` (very nomagic) is a literal string.
pub fn translate(pattern: &str) -> String {
    if let Some(rest) = pattern.strip_prefix("\\V") {
        return regex::escape(rest);
    }
    if let Some(rest) = pattern.strip_prefix("\\v") {
        return rest.replace("\\<", "\\b").replace("\\>", "\\b");
    }

    let mut out = String::with_capacity(pattern.len() + 8);
    let mut chars = pattern.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            // Magic mode treats these as literals; regex treats them as operators.
            if matches!(c, '+' | '?' | '(' | ')' | '{' | '}' | '|') {
                out.push('\\');
            }
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('<') | Some('>') => out.push_str("\\b"),
            // Backslashed in vim means "operator"; regex wants them bare.
            Some(esc @ ('+' | '?' | '(' | ')' | '{' | '}' | '|')) => out.push(esc),
            Some('c') => out.insert_str(0, "(?i)"),
            Some('C') => {}
            // Vim character-class aliases. Passing these through would silently mean something
            // else: `\a` is alphabetic in vim but BEL in `regex`, while `\l`, `\u` and `\x` are
            // not valid `regex` escapes at all, so the whole pattern would be dropped.
            Some('a') => out.push_str("[A-Za-z]"),
            Some('A') => out.push_str("[^A-Za-z]"),
            Some('l') => out.push_str("[a-z]"),
            Some('L') => out.push_str("[^a-z]"),
            Some('u') => out.push_str("[A-Z]"),
            Some('U') => out.push_str("[^A-Z]"),
            Some('d') => out.push_str("[0-9]"),
            Some('D') => out.push_str("[^0-9]"),
            Some('x') => out.push_str("[0-9A-Fa-f]"),
            Some('X') => out.push_str("[^0-9A-Fa-f]"),
            Some('o') => out.push_str("[0-7]"),
            Some('O') => out.push_str("[^0-7]"),
            Some('h') => out.push_str("[A-Za-z_]"),
            Some('H') => out.push_str("[^A-Za-z_]"),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push_str("\\\\"),
        }
    }
    out
}

pub struct Search {
    pub pattern: String,
    pub forward: bool,
    regex: regex::Regex,
}

impl Search {
    pub fn new(pattern: &str, forward: bool) -> Option<Self> {
        let regex = regex::Regex::new(&translate(pattern)).ok()?;
        Some(Self {
            pattern: pattern.to_string(),
            forward,
            regex,
        })
    }

    /// Every match in `text`, as byte ranges.
    pub fn matches(&self, text: &str) -> Vec<Range<usize>> {
        self.regex.find_iter(text).map(|m| m.range()).collect()
    }
}

/// The match to land on when stepping from `from`, wrapping around the ends of the buffer.
pub fn step(matches: &[Range<usize>], from: usize, forward: bool) -> Option<usize> {
    if matches.is_empty() {
        return None;
    }
    if forward {
        matches
            .iter()
            .find(|m| m.start > from)
            .or_else(|| matches.first())
            .map(|m| m.start)
    } else {
        matches
            .iter()
            .rev()
            .find(|m| m.start < from)
            .or_else(|| matches.last())
            .map(|m| m.start)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn magic_mode_flips_which_operators_need_a_backslash() {
        assert_eq!(translate("a\\+b"), "a+b");
        assert_eq!(translate("a+b"), "a\\+b");
        assert_eq!(translate("foo\\|bar"), "foo|bar");
        assert_eq!(translate("a.c"), "a.c");
    }

    #[test]
    fn word_boundaries_become_regex_boundaries() {
        assert_eq!(translate("\\<word\\>"), "\\bword\\b");
    }

    #[test]
    fn very_nomagic_escapes_everything() {
        assert_eq!(translate("\\Va.c+"), regex::escape("a.c+"));
    }

    #[test]
    fn very_magic_passes_operators_through() {
        assert_eq!(translate("\\v(a|b)+"), "(a|b)+");
    }

    #[test]
    fn case_insensitive_flag_is_hoisted() {
        assert_eq!(translate("foo\\c"), "(?i)foo");
    }

    /// `\a` used to reach `regex` as BEL and match the wrong thing; `\l`, `\u` and `\x` are not
    /// `regex` escapes at all, so `Search::new` dropped the pattern entirely.
    #[test]
    fn character_class_aliases_translate_rather_than_leak() {
        assert_eq!(translate("\\a"), "[A-Za-z]");
        assert_eq!(translate("\\l\\u"), "[a-z][A-Z]");
        assert_eq!(translate("\\d\\x"), "[0-9][0-9A-Fa-f]");
        assert_eq!(translate("\\h"), "[A-Za-z_]");

        assert!(Search::new("\\afoo", true).is_some());
        assert!(Search::new("\\x2", true).is_some());
    }

    #[test]
    fn stepping_wraps_at_both_ends() {
        let m = vec![2..5, 10..12];
        assert_eq!(step(&m, 0, true), Some(2));
        assert_eq!(step(&m, 2, true), Some(10));
        assert_eq!(step(&m, 10, true), Some(2));
        assert_eq!(step(&m, 12, false), Some(10));
        assert_eq!(step(&m, 2, false), Some(10));
        assert_eq!(step(&[], 0, true), None);
    }

    #[test]
    fn an_invalid_pattern_yields_no_search() {
        assert!(Search::new("\\v(unclosed", true).is_none());
    }
}
