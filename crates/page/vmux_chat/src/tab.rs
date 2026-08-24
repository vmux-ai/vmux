#[derive(Clone, PartialEq)]
pub struct Accent {
    pub css: String,
    pub rgb: String,
}

impl Accent {
    pub fn of_agent(profile_color: &str, agent: &str) -> Self {
        Self::resolve(
            profile_color,
            vmux_ui::agent_accent::agent_accent(agent).rain_rgb,
        )
    }

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
