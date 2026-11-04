use std::path::PathBuf;

use bevy::prelude::*;
use vmux_ecs::persistence::DurableStore;
#[cfg(test)]
use vmux_ecs::persistence::StoreLoad;
#[cfg(test)]
use vmux_ecs::profile::ProfilePaths;
use vmux_ecs::{Bookmark, Folder, Pin};

use crate::persistence::{STORE_NAME, STORE_SCHEMA_VERSION};

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
    pub(super) fn store(&self) -> DurableStore {
        DurableStore::new(&self.0, STORE_NAME, STORE_SCHEMA_VERSION)
    }

    #[cfg(test)]
    pub(super) fn load_world(&self) -> moonshine_save::prelude::LoadWorld<BookmarkFilter> {
        let StoreLoad::Ready(snapshot) = self.store().load().unwrap() else {
            panic!("bookmark store should be ready");
        };
        moonshine_save::prelude::LoadWorld::from_stream(std::io::Cursor::new(
            snapshot.into_payload().into_bytes(),
        ))
    }
}
