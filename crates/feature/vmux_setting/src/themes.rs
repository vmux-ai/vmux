use bevy::prelude::Resource;
use serde::{Deserialize, Serialize};
use vmux_api::terminal::{AnsiPalette, RgbColor};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TerminalColorScheme {
    pub name: String,
    #[serde(default)]
    pub light: Option<String>,
    #[serde(default)]
    pub dark: Option<String>,
    pub foreground: RgbColor,
    pub background: RgbColor,
    pub cursor: RgbColor,
    pub ansi: AnsiPalette,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct TerminalColorSchemeRon {
    name: String,
    #[serde(default)]
    light: Option<String>,
    #[serde(default)]
    dark: Option<String>,
    foreground: [u8; 3],
    background: [u8; 3],
    cursor: [u8; 3],
    ansi: [[u8; 3]; 16],
}

impl From<TerminalColorSchemeRon> for TerminalColorScheme {
    fn from(value: TerminalColorSchemeRon) -> Self {
        Self {
            name: value.name,
            light: value.light,
            dark: value.dark,
            foreground: value.foreground.into(),
            background: value.background.into(),
            cursor: value.cursor.into(),
            ansi: value.ansi.into(),
        }
    }
}

#[derive(Clone, Debug, Default, Resource)]
pub struct TerminalColorSchemes(Vec<TerminalColorScheme>);

impl TerminalColorSchemes {
    pub(crate) fn extend(&mut self, schemes: impl IntoIterator<Item = TerminalColorSchemeRon>) {
        for scheme in schemes {
            let scheme = TerminalColorScheme::from(scheme);
            if let Some(current) = self
                .0
                .iter_mut()
                .find(|current| current.name == scheme.name)
            {
                *current = scheme;
            } else {
                self.0.push(scheme);
            }
        }
        self.0.sort_by(|left, right| left.name.cmp(&right.name));
    }

    pub fn resolve(
        &self,
        name: &str,
        custom_themes: &[TerminalColorScheme],
    ) -> TerminalColorScheme {
        if let Some(theme) = custom_themes.iter().find(|theme| theme.name == name) {
            return theme.clone();
        }
        if let Some(theme) = self.0.iter().find(|theme| theme.name == name) {
            return theme.clone();
        }
        bevy::log::warn!("unknown terminal color scheme {name}; using the first registered scheme");
        self.0
            .first()
            .cloned()
            .expect("the settings feature manifest must register a terminal color scheme")
    }

    pub fn for_appearance(
        &self,
        scheme: TerminalColorScheme,
        dark: bool,
        custom_themes: &[TerminalColorScheme],
    ) -> TerminalColorScheme {
        let counterpart = match dark {
            true => scheme.dark.as_deref(),
            false => scheme.light.as_deref(),
        };
        let Some(counterpart) = counterpart else {
            return scheme;
        };
        self.resolve(counterpart, custom_themes)
    }
}
