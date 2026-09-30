use vmux_api::command_bar::SearchEngine;

pub(crate) struct SearchEngines;

impl SearchEngines {
    pub(crate) const ALL: [SearchEngine; 5] = [
        SearchEngine::Google,
        SearchEngine::Bing,
        SearchEngine::DuckDuckGo,
        SearchEngine::Brave,
        SearchEngine::Kagi,
    ];

    #[cfg(ui)]
    pub(crate) const fn name(engine: SearchEngine) -> &'static str {
        match engine {
            SearchEngine::Google => "Google",
            SearchEngine::Bing => "Bing",
            SearchEngine::DuckDuckGo => "DuckDuckGo",
            SearchEngine::Brave => "Brave Search",
            SearchEngine::Kagi => "Kagi",
        }
    }

    #[cfg(any(host, test))]
    pub(crate) fn from_url(url: &str) -> Option<SearchEngine> {
        let parsed = url::Url::parse(url).ok()?;
        let host = parsed.host_str()?.trim_start_matches("www.");
        match host {
            "google.com" => Some(SearchEngine::Google),
            "bing.com" => Some(SearchEngine::Bing),
            "duckduckgo.com" => Some(SearchEngine::DuckDuckGo),
            "search.brave.com" => Some(SearchEngine::Brave),
            "kagi.com" => Some(SearchEngine::Kagi),
            _ => None,
        }
    }

    pub(crate) fn url(engine: SearchEngine, query: &str) -> String {
        let query: String = url::form_urlencoded::byte_serialize(query.trim().as_bytes()).collect();
        match engine {
            SearchEngine::Google => format!("https://www.google.com/search?q={query}"),
            SearchEngine::Bing => format!("https://www.bing.com/search?q={query}"),
            SearchEngine::DuckDuckGo => format!("https://duckduckgo.com/?q={query}"),
            SearchEngine::Brave => format!("https://search.brave.com/search?q={query}"),
            SearchEngine::Kagi => format!("https://kagi.com/search?q={query}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engines_build_encoded_urls() {
        assert_eq!(
            SearchEngines::url(SearchEngine::Google, "hello world"),
            "https://www.google.com/search?q=hello+world"
        );
        assert_eq!(
            SearchEngines::url(SearchEngine::Bing, "hello world"),
            "https://www.bing.com/search?q=hello+world"
        );
        assert_eq!(
            SearchEngines::url(SearchEngine::DuckDuckGo, "hello world"),
            "https://duckduckgo.com/?q=hello+world"
        );
        assert_eq!(
            SearchEngines::url(SearchEngine::Brave, "hello world"),
            "https://search.brave.com/search?q=hello+world"
        );
        assert_eq!(
            SearchEngines::url(SearchEngine::Kagi, "hello world"),
            "https://kagi.com/search?q=hello+world"
        );
    }
}
