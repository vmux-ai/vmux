//! The colour a conversation paints itself with.
//!
//! Ungated, because the host derives the same colour for the tab as the page does for its own
//! chrome, and the two disagreeing would be visible.

/// The colour a conversation paints itself with: the profile's, when that is a usable hex, else
/// the agent's built-in fallback.
#[derive(Clone, PartialEq)]
pub struct Accent {
    /// A CSS colour, for `--agent-accent` and the generated favicon.
    pub css: String,
    /// The same colour as space-separated `r g b` channels, for the install backdrop.
    pub rgb: String,
}

impl Accent {
    /// The colour a conversation paints itself with, falling back to the agent's own.
    ///
    /// Lives here rather than at either call site because the tab and the page's own chrome must
    /// agree, and because `vmux_agent` does not link `vmux_ui` off a `ui` target — this crate
    /// does, so the fallback lookup stays on this side of the seam.
    pub fn of_agent(profile_color: &str, agent: &str) -> Self {
        Self::resolve(
            profile_color,
            vmux_ui::agent_accent::agent_accent(agent).rain_rgb,
        )
    }

    /// Resolve a profile colour against the agent's fallback, so both forms agree on which one won.
    pub fn resolve(profile_color: &str, fallback_rgb: &str) -> Self {
        let Some((red, green, blue)) = Self::channels(profile_color) else {
            return Accent {
                css: format!("rgb({fallback_rgb})"),
                rgb: fallback_rgb.to_string(),
            };
        };
        Accent {
            css: profile_color.to_string(),
            rgb: format!("{red} {green} {blue}"),
        }
    }

    fn channels(color: &str) -> Option<(u8, u8, u8)> {
        let hex = color.strip_prefix('#')?;
        if hex.len() != 6 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return None;
        }
        Some((
            u8::from_str_radix(&hex[0..2], 16).ok()?,
            u8::from_str_radix(&hex[2..4], 16).ok()?,
            u8::from_str_radix(&hex[4..6], 16).ok()?,
        ))
    }
}
