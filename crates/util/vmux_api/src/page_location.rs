#[vmux_api::ui_event(Eq)]
pub struct PageLocationRequest {
    pub fragment: String,
}

impl PageLocationRequest {
    pub fn location(&self) -> Option<PageLocationUiState> {
        if self.fragment.len() > 2048
            || self.fragment.contains('#')
            || self.fragment.chars().any(char::is_whitespace)
            || self.fragment.chars().any(char::is_control)
        {
            return None;
        }
        Some(PageLocationUiState {
            fragment: self.fragment.clone(),
        })
    }
}

#[vmux_api::ui_state(Default, Eq)]
pub struct PageLocationUiState {
    pub fragment: String,
}

impl PageLocationUiState {
    pub fn from_url(url: &str) -> Self {
        Self {
            fragment: url
                .split_once('#')
                .map(|(_, fragment)| fragment)
                .unwrap_or_default()
                .to_string(),
        }
    }

    pub fn url(&self, current: &str) -> String {
        let base = current.split('#').next().unwrap_or(current);
        if self.fragment.is_empty() {
            return base.to_string();
        }
        format!("{base}#{}", self.fragment)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changing_location_preserves_the_document_and_query() {
        let location = PageLocationRequest {
            fragment: "appearance".to_string(),
        }
        .location()
        .unwrap();
        let url = location.url("vmux://settings/?section=general#old");
        assert_eq!(url, "vmux://settings/?section=general#appearance");
        assert_eq!(PageLocationUiState::from_url(&url), location);
        assert_eq!(
            PageLocationUiState::default().url(&url),
            "vmux://settings/?section=general"
        );
    }

    #[test]
    fn location_requests_reject_non_fragment_input() {
        for fragment in ["appearance#other", "appearance\nother", "has space"] {
            assert!(
                PageLocationRequest {
                    fragment: fragment.to_string()
                }
                .location()
                .is_none()
            );
        }
    }
}
