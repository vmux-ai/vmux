use std::ops::Range;

pub struct Search {
    pub pattern: String,
    pub forward: bool,
    regex: regex::Regex,
}

impl Search {
    pub fn new(pattern: &str, forward: bool) -> Option<Self> {
        let regex = regex::Regex::new(&Self::translate(pattern)).ok()?;
        Some(Self {
            pattern: pattern.to_string(),
            forward,
            regex,
        })
    }

    pub fn matches(&self, text: &str) -> Vec<Range<usize>> {
        self.regex.find_iter(text).map(|m| m.range()).collect()
    }

    pub(crate) fn translate(pattern: &str) -> String {
        if let Some(rest) = pattern.strip_prefix("\\V") {
            return regex::escape(rest);
        }
        if let Some(rest) = pattern.strip_prefix("\\v") {
            return rest.replace("\\<", "\\b").replace("\\>", "\\b");
        }

        let mut output = String::with_capacity(pattern.len() + 8);
        let mut chars = pattern.chars().peekable();
        while let Some(character) = chars.next() {
            if character != '\\' {
                if matches!(character, '+' | '?' | '(' | ')' | '{' | '}' | '|') {
                    output.push('\\');
                }
                output.push(character);
                continue;
            }
            match chars.next() {
                Some('<') | Some('>') => output.push_str("\\b"),
                Some(escaped @ ('+' | '?' | '(' | ')' | '{' | '}' | '|')) => output.push(escaped),
                Some('c') => output.insert_str(0, "(?i)"),
                Some('C') => {}
                Some('a') => output.push_str("[A-Za-z]"),
                Some('A') => output.push_str("[^A-Za-z]"),
                Some('l') => output.push_str("[a-z]"),
                Some('L') => output.push_str("[^a-z]"),
                Some('u') => output.push_str("[A-Z]"),
                Some('U') => output.push_str("[^A-Z]"),
                Some('d') => output.push_str("[0-9]"),
                Some('D') => output.push_str("[^0-9]"),
                Some('x') => output.push_str("[0-9A-Fa-f]"),
                Some('X') => output.push_str("[^0-9A-Fa-f]"),
                Some('o') => output.push_str("[0-7]"),
                Some('O') => output.push_str("[^0-7]"),
                Some('h') => output.push_str("[A-Za-z_]"),
                Some('H') => output.push_str("[^A-Za-z_]"),
                Some(other) => {
                    output.push('\\');
                    output.push(other);
                }
                None => output.push_str("\\\\"),
            }
        }
        output
    }

    pub(crate) fn step(matches: &[Range<usize>], from: usize, forward: bool) -> Option<usize> {
        if matches.is_empty() {
            return None;
        }
        if forward {
            matches
                .iter()
                .find(|range| range.start > from)
                .or_else(|| matches.first())
                .map(|range| range.start)
        } else {
            matches
                .iter()
                .rev()
                .find(|range| range.start < from)
                .or_else(|| matches.last())
                .map(|range| range.start)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn magic_mode_flips_which_operators_need_a_backslash() {
        assert_eq!(Search::translate("a\\+b"), "a+b");
        assert_eq!(Search::translate("a+b"), "a\\+b");
        assert_eq!(Search::translate("foo\\|bar"), "foo|bar");
        assert_eq!(Search::translate("a.c"), "a.c");
    }

    #[test]
    fn word_boundaries_become_regex_boundaries() {
        assert_eq!(Search::translate("\\<word\\>"), "\\bword\\b");
    }

    #[test]
    fn very_nomagic_escapes_everything() {
        assert_eq!(Search::translate("\\Va.c+"), regex::escape("a.c+"));
    }

    #[test]
    fn very_magic_passes_operators_through() {
        assert_eq!(Search::translate("\\v(a|b)+"), "(a|b)+");
    }

    #[test]
    fn case_insensitive_flag_is_hoisted() {
        assert_eq!(Search::translate("foo\\c"), "(?i)foo");
    }

    #[test]
    fn character_class_aliases_translate_rather_than_leak() {
        assert_eq!(Search::translate("\\a"), "[A-Za-z]");
        assert_eq!(Search::translate("\\l\\u"), "[a-z][A-Z]");
        assert_eq!(Search::translate("\\d\\x"), "[0-9][0-9A-Fa-f]");
        assert_eq!(Search::translate("\\h"), "[A-Za-z_]");

        assert!(Search::new("\\afoo", true).is_some());
        assert!(Search::new("\\x2", true).is_some());
    }

    #[test]
    fn stepping_wraps_at_both_ends() {
        let m = vec![2..5, 10..12];
        assert_eq!(Search::step(&m, 0, true), Some(2));
        assert_eq!(Search::step(&m, 2, true), Some(10));
        assert_eq!(Search::step(&m, 10, true), Some(2));
        assert_eq!(Search::step(&m, 12, false), Some(10));
        assert_eq!(Search::step(&m, 2, false), Some(10));
        assert_eq!(Search::step(&[], 0, true), None);
    }

    #[test]
    fn an_invalid_pattern_yields_no_search() {
        assert!(Search::new("\\v(unclosed", true).is_none());
    }
}
