use std::path::Path;

pub struct UrlPresentation<'a> {
    canonical: &'a str,
    text: String,
    url: Option<url::Url>,
    segments: Vec<Segment>,
}

struct Segment {
    start: usize,
    end: usize,
    value: String,
    embedded_file: bool,
}

impl<'a> UrlPresentation<'a> {
    pub fn new(canonical: &'a str) -> Self {
        let mut presentation = Self {
            canonical,
            text: canonical.to_string(),
            url: None,
            segments: Vec::new(),
        };
        let Ok(url) = url::Url::parse(canonical) else {
            return presentation;
        };
        if !matches!(url.scheme(), "vmux" | "file" | "http" | "https") || url.cannot_be_a_base() {
            return presentation;
        }
        let Some(parts) = url.path_segments() else {
            return presentation;
        };
        let mut text = url[..url::Position::BeforePath].to_string();
        if url.path().starts_with('/') {
            text.push('/');
        }
        for (index, part) in parts.enumerate() {
            if index > 0 {
                text.push('/');
            }
            let value = percent_encoding::percent_decode_str(part)
                .decode_utf8()
                .map(|value| value.into_owned())
                .unwrap_or_else(|_| part.to_string());
            let file = url::Url::parse(&value)
                .ok()
                .filter(|url| url.scheme() == "file")
                .and_then(|url| url.to_file_path().ok());
            let mut decoded = file
                .as_ref()
                .map(|path| format!("file://{}", path.to_string_lossy()))
                .unwrap_or_else(|| value.clone());
            let mut embedded_file = file.is_some();
            if decoded.chars().any(char::is_control) {
                decoded = part.to_string();
                embedded_file = false;
            }
            let start = text.len();
            text.push_str(&decoded);
            presentation.segments.push(Segment {
                start,
                end: text.len(),
                value,
                embedded_file,
            });
        }
        text.push_str(&url[url::Position::AfterPath..]);
        presentation.text = text;
        presentation.url = Some(url);
        presentation
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn ghost(&self, suffix: &str) -> String {
        let full = format!("{}{suffix}", self.canonical);
        let display = UrlPresentation::new(&full);
        display
            .text()
            .strip_prefix(self.text())
            .unwrap_or_default()
            .to_string()
    }

    pub fn edit(&self, edited: &str) -> String {
        if edited == self.text || edited == self.canonical {
            return self.canonical.to_string();
        }
        let Some(mut url) = self.url.clone() else {
            return edited.to_string();
        };
        if edited.contains('%')
            && let Ok(pasted) = url::Url::parse(edited)
            && pasted.as_str() == edited
            && pasted
                .path_segments()
                .is_some_and(|parts| parts.count() == self.segments.len())
        {
            return edited.to_string();
        }
        let prefix = self
            .text
            .chars()
            .zip(edited.chars())
            .take_while(|(left, right)| left == right)
            .map(|(character, _)| character.len_utf8())
            .sum::<usize>();
        let suffix = self.text[prefix..]
            .chars()
            .rev()
            .zip(edited[prefix..].chars().rev())
            .take_while(|(left, right)| left == right)
            .map(|(character, _)| character.len_utf8())
            .sum::<usize>();
        let old_end = self.text.len() - suffix;
        let new_end = edited.len() - suffix;
        let Some(index) = self
            .segments
            .iter()
            .position(|segment| segment.start <= prefix && old_end <= segment.end)
        else {
            let authority_end = self
                .segments
                .first()
                .map(|segment| segment.start.saturating_sub(1))
                .unwrap_or(self.text.len());
            if old_end <= authority_end {
                let mut authority = self.text[..authority_end].to_string();
                authority.replace_range(prefix..old_end, &edited[prefix..new_end]);
                return url::Url::parse(&format!(
                    "{authority}{}",
                    &url[url::Position::BeforePath..]
                ))
                .map(|url| url.to_string())
                .unwrap_or_else(|_| edited.to_string());
            }
            let path_end = self
                .segments
                .last()
                .map(|segment| segment.end)
                .unwrap_or(authority_end);
            if prefix >= path_end {
                let mut suffix = self.text[path_end..].to_string();
                suffix.replace_range(
                    prefix - path_end..old_end - path_end,
                    &edited[prefix..new_end],
                );
                return url::Url::parse(&format!("{}{suffix}", &url[..url::Position::AfterPath]))
                    .map(|url| url.to_string())
                    .unwrap_or_else(|_| edited.to_string());
            }
            return edited.to_string();
        };
        let segment = &self.segments[index];
        let mut changed = self.text[segment.start..segment.end].to_string();
        changed.replace_range(
            prefix - segment.start..old_end - segment.start,
            &edited[prefix..new_end],
        );
        let value = if segment.embedded_file {
            let Some(path) = changed.strip_prefix("file://") else {
                return edited.to_string();
            };
            let Ok(url) = url::Url::from_file_path(Path::new(path)) else {
                return edited.to_string();
            };
            url.to_string()
        } else {
            changed
        };
        let mut parts = self
            .segments
            .iter()
            .map(|segment| segment.value.clone())
            .collect::<Vec<_>>();
        parts[index] = value;
        if let Ok(mut target) = url.path_segments_mut() {
            target.clear().extend(parts);
        }
        url.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_file_paths_and_branches_display_readably_and_round_trip_after_edits() {
        let file = url::Url::from_file_path("/tmp/a project/日本語%2F").unwrap();
        let mut canonical = url::Url::parse("vmux://git/").unwrap();
        canonical.path_segments_mut().unwrap().clear().extend([
            "branch-changes",
            file.as_str(),
            "feature/topic",
        ]);
        let presentation = UrlPresentation::new(canonical.as_str());
        assert_eq!(
            presentation.text(),
            "vmux://git/branch-changes/file:///tmp/a project/日本語%2F/feature/topic"
        );
        assert_eq!(presentation.edit(presentation.text()), canonical.as_str());
        let edited =
            presentation.edit(&presentation.text().replace("feature/topic", "feature/new"));
        let parsed = url::Url::parse(&edited).unwrap();
        let parts = parsed.path_segments().unwrap().collect::<Vec<_>>();
        assert_eq!(
            percent_encoding::percent_decode_str(parts[1])
                .decode_utf8()
                .unwrap(),
            file.as_str()
        );
        assert_eq!(
            percent_encoding::percent_decode_str(parts[2])
                .decode_utf8()
                .unwrap(),
            "feature/new"
        );
        let moved = presentation.edit(&presentation.text().replace("a project", "another project"));
        let parsed = url::Url::parse(&moved).unwrap();
        let file =
            percent_encoding::percent_decode_str(parsed.path_segments().unwrap().nth(1).unwrap())
                .decode_utf8()
                .unwrap();
        assert_eq!(
            url::Url::parse(&file).unwrap().to_file_path().unwrap(),
            Path::new("/tmp/another project/日本語%2F")
        );
    }

    #[test]
    fn plain_queries_and_canonical_pastes_preserve_their_meaning() {
        assert_eq!(
            UrlPresentation::new("find %20 notes").edit("find %20 other notes"),
            "find %20 other notes"
        );
        let presentation = UrlPresentation::new("https://example.com/a%20b");
        assert_eq!(
            presentation.edit("https://other.com/a%20b"),
            "https://other.com/a%20b"
        );
        assert_eq!(
            UrlPresentation::new("https://example.com/%0A").text(),
            "https://example.com/%0A"
        );
    }
}
