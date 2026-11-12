use std::path::PathBuf;

use bevy::prelude::*;
use moonshine_save::prelude::LoadWorld;
#[cfg(test)]
use vmux_ecs::profile::ProfilePaths;
use vmux_ecs::{Bookmark, Folder, Pin};

pub(super) type BookmarkFilter = Or<(With<Pin>, With<Bookmark>, With<Folder>)>;

#[derive(Component)]
pub(super) struct BookmarkStorePath(pub(super) PathBuf);

#[cfg(test)]
impl Default for BookmarkStorePath {
    fn default() -> Self {
        Self(ProfilePaths::current().profile().join("bookmarks.ron"))
    }
}

impl BookmarkStorePath {
    pub(super) fn load(&self) -> std::io::Result<LoadWorld<BookmarkFilter>> {
        let mut source = std::fs::read_to_string(&self.0)?;
        for (legacy, current) in [
            (
                "vmux_desktop::bookmark_persistence::OfferedBookmarkDefaults",
                "vmux_bookmark::OfferedBookmarkDefaults",
            ),
            ("vmux_core::BookmarkOrder", "vmux_ecs::BookmarkOrder"),
            ("vmux_core::Pin", "vmux_ecs::Pin"),
            ("vmux_core::Bookmark", "vmux_ecs::Bookmark"),
            ("vmux_core::Folder", "vmux_ecs::Folder"),
            ("vmux_core::Collapsed", "vmux_ecs::Collapsed"),
            ("vmux_core::Uuid", "vmux_ecs::Uuid"),
        ] {
            source = source.replace(legacy, current);
        }
        if let Some((normalized, unknown)) = vmux_api::icon::StoredPageIcons(&source).normalize() {
            warn!(
                ?unknown,
                "Replacing unavailable bookmark icons while loading"
            );
            source = normalized;
        }
        Ok(LoadWorld::<BookmarkFilter>::from_stream(
            std::io::Cursor::new(source.into_bytes()),
        ))
    }
}
