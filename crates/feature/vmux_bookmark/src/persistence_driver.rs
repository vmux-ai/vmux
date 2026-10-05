use std::path::PathBuf;

use bevy::prelude::*;
use bevy_world_serialization::WorldFilter;
use moonshine_save::prelude::LoadWorld;
use vmux_api::bookmark::SmartBookmarkFolder;
#[cfg(test)]
use vmux_ecs::profile::ProfilePaths;
use vmux_ecs::{Bookmark, BookmarkOrder, Collapsed, Folder, PageMetadata, Pin, Uuid};

use super::persistence::OfferedBookmarkDefaults;

pub(super) type BookmarkFilter = Or<(With<Pin>, With<Bookmark>, With<Folder>)>;

#[derive(Component)]
pub(super) struct BookmarkPersistencePath(pub(super) PathBuf);

#[cfg(test)]
impl Default for BookmarkPersistencePath {
    fn default() -> Self {
        Self(ProfilePaths::current().profile().join("bookmarks.ron"))
    }
}

impl BookmarkPersistencePath {
    pub(super) fn scene_filter() -> WorldFilter {
        WorldFilter::deny_all()
            .allow::<ChildOf>()
            .allow::<Children>()
            .allow::<Name>()
            .allow::<Pin>()
            .allow::<Bookmark>()
            .allow::<Folder>()
            .allow::<SmartBookmarkFolder>()
            .allow::<Collapsed>()
            .allow::<Uuid>()
            .allow::<BookmarkOrder>()
            .allow::<PageMetadata>()
    }

    pub(super) fn resource_filter() -> WorldFilter {
        WorldFilter::deny_all().allow::<OfferedBookmarkDefaults>()
    }

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
        Ok(LoadWorld::<BookmarkFilter>::from_stream(
            std::io::Cursor::new(source.into_bytes()),
        ))
    }
}
