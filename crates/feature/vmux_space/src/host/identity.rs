use bevy::ecs::system::SystemParam;
use bevy::prelude::{Name, Query, With};
use vmux_layout::space::{Space, SpaceId};

const PLANETS: &[&str] = &[
    "mercury", "venus", "earth", "mars", "jupiter", "saturn", "uranus", "neptune", "ceres",
    "pluto", "haumea", "makemake", "eris",
];

#[derive(SystemParam)]
pub(super) struct SpaceIdentity<'w, 's> {
    ids: Query<'w, 's, &'static SpaceId, With<Space>>,
    names: Query<'w, 's, &'static Name, With<Space>>,
}

impl SpaceIdentity<'_, '_> {
    pub(super) fn bootstrap_id() -> SpaceId {
        SpaceId(crate::model::BOOTSTRAP_SPACE_ID.into())
    }

    pub(super) fn suggested(&self, excluded: &[&str]) -> String {
        let start = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.subsec_nanos() as usize)
            .unwrap_or_default()
            % PLANETS.len();
        for offset in 0..PLANETS.len() {
            let name = PLANETS[(start + offset) % PLANETS.len()];
            if !excluded.contains(&name)
                && !self.names.iter().any(|existing| existing.as_str() == name)
            {
                return name.to_string();
            }
        }
        let base = PLANETS[start];
        for suffix in 2usize.. {
            let candidate = format!("{base}-{suffix}");
            if !excluded.contains(&candidate.as_str())
                && !self.names.iter().any(|name| name.as_str() == candidate)
            {
                return candidate;
            }
        }
        unreachable!()
    }

    pub(super) fn unique(&self, name: &str) -> SpaceId {
        let base = SpaceId::normalized(name);
        if !self.ids.iter().any(|existing| existing == &base) {
            return base;
        }
        for suffix in 2usize.. {
            let candidate = SpaceId(format!("{}-{suffix}", base.0));
            if !self.ids.iter().any(|existing| existing == &candidate) {
                return candidate;
            }
        }
        unreachable!()
    }
}
