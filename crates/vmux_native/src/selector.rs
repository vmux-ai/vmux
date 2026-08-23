//! The subset of CSS a test needs to name an element.
//!
//! Deliberately not a CSS engine. A harness picks elements by test id, by tag, or by a single
//! attribute, and every selector below is one of those three spelled the way CSS spells it. The
//! moment a query needs a combinator it wants a different tool, not a bigger parser here.

use std::fmt;

/// One element, named the way a test would name it.
///
/// Parsed from `tag`, `#id`, `.class`, `[name=value]`, or a tag followed by one of the others.
#[derive(Clone, Debug, PartialEq)]
pub struct Selector {
    tag: Option<String>,
    filter: Option<Filter>,
}

/// What narrows a tag down to one element.
#[derive(Clone, Debug, PartialEq)]
enum Filter {
    /// An attribute equal to a value.
    Attribute { name: String, value: String },
    /// A token in the space-separated `class` attribute.
    Class(String),
}

/// Why a selector string was not a selector.
#[derive(Clone, Debug, PartialEq)]
pub enum SelectorError {
    /// The string was empty, or a sigil was given nothing to name.
    Empty,
    /// A `[` was opened and never closed.
    Unclosed,
    /// A bracket held no `=`, so it named an attribute but not a value.
    NoValue,
}

impl fmt::Display for SelectorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("a selector cannot be empty"),
            Self::Unclosed => f.write_str("a `[` must be closed by a `]`"),
            Self::NoValue => f.write_str("`[name=value]` needs the `=value`"),
        }
    }
}

impl std::error::Error for SelectorError {}

impl std::str::FromStr for Selector {
    type Err = SelectorError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let text = text.trim();
        if text.is_empty() {
            return Err(SelectorError::Empty);
        }

        let (tag, rest) = Self::split_tag(text);
        if rest.is_empty() {
            return match tag {
                Some(tag) => Ok(Self {
                    tag: Some(tag),
                    filter: None,
                }),
                None => Err(SelectorError::Empty),
            };
        }

        Ok(Self {
            tag,
            filter: Some(Filter::parse(rest)?),
        })
    }
}

impl Selector {
    /// `[data-testid=<id>]`, which is how a page marks something for a test to find.
    pub fn test_id(id: impl Into<String>) -> Self {
        Self {
            tag: None,
            filter: Some(Filter::Attribute {
                name: "data-testid".to_string(),
                value: id.into(),
            }),
        }
    }

    /// Whether an element with this tag and these attributes is the one being named.
    ///
    /// `attribute` answers for the element under test; a missing attribute is `None`.
    pub fn matches(&self, tag: &str, attribute: impl Fn(&str) -> Option<String>) -> bool {
        if let Some(wanted) = &self.tag
            && wanted != tag
        {
            return false;
        }

        let Some(filter) = &self.filter else {
            return true;
        };

        filter.matches(attribute)
    }

    /// The tag, and whatever follows it, given that a sigil or a `[` ends the tag.
    fn split_tag(text: &str) -> (Option<String>, &str) {
        let end = text.find(['#', '.', '[']).unwrap_or(text.len());
        let (tag, rest) = text.split_at(end);
        let tag = match tag.is_empty() {
            true => None,
            false => Some(tag.to_string()),
        };

        (tag, rest)
    }
}

impl Filter {
    fn parse(text: &str) -> Result<Self, SelectorError> {
        if let Some(id) = text.strip_prefix('#') {
            return match id.is_empty() {
                true => Err(SelectorError::Empty),
                false => Ok(Self::Attribute {
                    name: "id".to_string(),
                    value: id.to_string(),
                }),
            };
        }

        if let Some(class) = text.strip_prefix('.') {
            return match class.is_empty() {
                true => Err(SelectorError::Empty),
                false => Ok(Self::Class(class.to_string())),
            };
        }

        let Some(inner) = text.strip_prefix('[').and_then(|t| t.strip_suffix(']')) else {
            return Err(SelectorError::Unclosed);
        };
        let Some((name, value)) = inner.split_once('=') else {
            return Err(SelectorError::NoValue);
        };
        let name = name.trim();
        if name.is_empty() {
            return Err(SelectorError::Empty);
        }

        Ok(Self::Attribute {
            name: name.to_string(),
            value: Self::unquote(value.trim()).to_string(),
        })
    }

    fn matches(&self, attribute: impl Fn(&str) -> Option<String>) -> bool {
        match self {
            Self::Attribute { name, value } => attribute(name).as_deref() == Some(value.as_str()),
            Self::Class(wanted) => {
                let Some(classes) = attribute("class") else {
                    return false;
                };

                classes.split_whitespace().any(|class| class == wanted)
            }
        }
    }

    fn unquote(value: &str) -> &str {
        for quote in ['"', '\''] {
            if let Some(inner) = value
                .strip_prefix(quote)
                .and_then(|v| v.strip_suffix(quote))
            {
                return inner;
            }
        }

        value
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    /// A class attribute holds many classes, and `.a` must not match the element whose class is
    /// `ab` — the usual bug when a selector reaches for `contains`.
    #[test]
    fn a_class_matches_a_whole_token_rather_than_a_substring() {
        let selector: Selector = ".row".parse().unwrap();

        assert!(selector.matches("div", attributes(&[("class", "row tall")])));
        assert!(!selector.matches("div", attributes(&[("class", "rowdy")])));
    }

    #[test]
    fn a_tag_narrows_an_attribute_match() {
        let selector: Selector = "button[data-testid=send]".parse().unwrap();

        assert!(selector.matches("button", attributes(&[("data-testid", "send")])));
        assert!(
            !selector.matches("a", attributes(&[("data-testid", "send")])),
            "the tag is part of the selector, not decoration"
        );
    }

    /// The value a page writes may itself be quoted, and the quotes are the selector's, not the
    /// value's.
    #[test]
    fn a_quoted_value_matches_the_unquoted_attribute() {
        let selector: Selector = "[href=\"/spaces\"]".parse().unwrap();

        assert!(selector.matches("a", attributes(&[("href", "/spaces")])));
    }

    #[test]
    fn an_absent_attribute_does_not_match() {
        let selector = Selector::test_id("send");

        assert!(!selector.matches("button", attributes(&[])));
    }

    #[test]
    fn a_bare_tag_matches_every_element_with_that_tag() {
        let selector: Selector = "button".parse().unwrap();

        assert!(selector.matches("button", attributes(&[])));
        assert!(!selector.matches("div", attributes(&[])));
    }

    #[test]
    fn a_selector_that_names_nothing_is_rejected() {
        for text in ["", "   ", "#", ".", "[]", "[name]", "[name=value"] {
            assert!(
                text.parse::<Selector>().is_err(),
                "{text:?} names no element and must not parse"
            );
        }
    }

    fn attributes(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();

        move |name: &str| map.get(name).cloned()
    }
}
