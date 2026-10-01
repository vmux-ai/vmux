use bevy::prelude::*;
use vmux_core::agent::AgentRequestAppExt;
use vmux_layout::bookmark::{
    AddRequest, CreateFolderRequest, PinRequest, PinUrlRequest, RemoveRequest, UnpinRequest,
};

pub struct BookmarkPlugin;

impl Plugin for BookmarkPlugin {
    fn build(&self, app: &mut App) {
        app.add_agent_message::<AddRequest>()
            .add_agent_message::<CreateFolderRequest>()
            .add_agent_message::<PinRequest>()
            .add_agent_message::<PinUrlRequest>()
            .add_agent_message::<RemoveRequest>()
            .add_agent_message::<UnpinRequest>()
            .add_plugins((
                crate::persistence::BookmarkPersistencePlugin,
                crate::tool::BookmarkToolPlugin,
            ));

        #[cfg(target_os = "macos")]
        app.add_plugins(macos::BookmarkMenuPlugin);
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use bevy::ecs::relationship::Relationship;
    use bevy::ecs::system::{NonSendMarker, SystemParam};
    use bevy::prelude::*;
    use std::collections::HashSet;
    use vmux_api::bookmark::{
        BookmarkFolderCreateEffect, BookmarkMenuEffect, BookmarkRenameEffect,
    };
    use vmux_core::{Bookmark, Collapsed, Folder, PageMetadata, Pin, Uuid, host::UiStateWrite};
    use vmux_layout::bookmark::{
        AddRequest, BookmarkMenuTarget, MoveFolderRequest, MoveRequest, PinRequest,
        RemoveFolderRequest, RemoveRequest, ShowBookmarkMenuRequest, ToggleFolderRequest,
        UnpinRequest,
    };
    use vmux_layout::stack::OpenRequest;
    use vmux_layout::state::LayoutUiState;
    use vmux_native::menu::{
        OsContextMenu, OsMenuEntry, OsMenuSelection, OsMenuSeparator, OsMenuSet,
    };
    use vmux_ui::i18n::{Locale, TranslationValue};

    pub(super) struct BookmarkMenuPlugin;

    impl Plugin for BookmarkMenuPlugin {
        fn build(&self, app: &mut App) {
            app.add_plugins(vmux_layout::LayoutContractPlugin)
                .add_message::<NewFolderInputRequest>()
                .add_message::<RenameInputRequest>()
                .add_systems(Startup, spawn_input_revision)
                .add_systems(
                    Update,
                    (
                        forward_message::<OpenRequest>,
                        forward_message::<AddRequest>,
                        forward_message::<MoveRequest>,
                        forward_message::<MoveFolderRequest>,
                        forward_message::<PinRequest>,
                        forward_message::<RemoveRequest>,
                        forward_message::<RemoveFolderRequest>,
                        forward_message::<ToggleFolderRequest>,
                        forward_message::<UnpinRequest>,
                        forward_message::<NewFolderInputRequest>,
                        forward_message::<RenameInputRequest>,
                    )
                        .in_set(OsMenuSet::Dispatch),
                )
                .add_systems(Update, show)
                .add_systems(
                    Update,
                    begin_input.after(vmux_layout::bookmark::BookmarkRequestSet),
                );
        }
    }

    #[derive(Component, Default)]
    struct BookmarkMenuInputRevision(u64);

    #[derive(Component)]
    struct BookmarkMenuMessage<M: Message>(M);

    #[derive(Clone)]
    struct FolderMenuRow {
        entity: Entity,
        uuid: String,
        name: String,
        parent: Option<Entity>,
    }

    type BookmarkMenuEntries<'w, 's> = Query<
        'w,
        's,
        (
            Entity,
            &'static Uuid,
            Option<&'static Name>,
            Option<&'static PageMetadata>,
            Has<Pin>,
            Has<Bookmark>,
            Has<Folder>,
            Has<Collapsed>,
            Option<&'static ChildOf>,
        ),
    >;

    #[derive(SystemParam)]
    struct BookmarkMenuBuilder<'w, 's> {
        focused: vmux_layout::window::FocusedWindow<'w, 's>,
        entries: BookmarkMenuEntries<'w, 's>,
        settings: Res<'w, vmux_setting::AppSettings>,
        context_menus: Query<'w, 's, Entity, With<OsContextMenu>>,
        commands: Commands<'w, 's>,
    }

    #[derive(Message, Clone)]
    struct NewFolderInputRequest {
        webview: Entity,
        parent: Option<String>,
    }

    #[derive(Message, Clone)]
    struct RenameInputRequest {
        webview: Entity,
        uuid: String,
    }

    fn show(
        _non_send: NonSendMarker,
        mut reader: MessageReader<ShowBookmarkMenuRequest>,
        mut builder: BookmarkMenuBuilder,
    ) {
        use bevy::winit::WINIT_WINDOWS;
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};

        let Some(request) = reader.read().last().cloned() else {
            return;
        };

        let Some(window_entity) = builder.focused.entity() else {
            return;
        };
        let view_ptr = WINIT_WINDOWS.with_borrow(|windows| {
            let id = windows.entity_to_winit.get(&window_entity)?;
            let wrapper = windows.windows.get(id)?;
            let handle = wrapper.window_handle().ok()?;
            match handle.as_raw() {
                RawWindowHandle::AppKit(handle) => Some(handle.ns_view.as_ptr()),
                _ => None,
            }
        });
        let Some(view_ptr) = view_ptr else {
            return;
        };

        for entity in &builder.context_menus {
            builder.commands.entity(entity).despawn();
        }
        let locale = Locale::requested(Some(&builder.settings.appearance.locale));
        let folders = FolderMenuRow::collect(&builder.entries);
        let menu = builder.commands.spawn(OsContextMenu::new(view_ptr)).id();
        match request.target {
            BookmarkMenuTarget::Root => {
                root_menu(menu, &mut builder.commands, &locale, request.webview)
            }
            BookmarkMenuTarget::Pin { uuid } => pin_menu(
                menu,
                &mut builder.commands,
                &locale,
                &builder.entries,
                &uuid,
            ),
            BookmarkMenuTarget::Entry { uuid } => bookmark_menu(
                menu,
                &mut builder.commands,
                &locale,
                &builder.entries,
                &folders,
                &uuid,
                request.webview,
            ),
            BookmarkMenuTarget::Folder { uuid, active_page } => folder_menu(
                menu,
                &mut builder.commands,
                &locale,
                &builder.entries,
                &folders,
                &uuid,
                active_page,
                request.webview,
            ),
        }
    }

    fn root_menu(menu: Entity, commands: &mut Commands, locale: &Locale, webview: Entity) {
        commands.spawn((
            OsMenuEntry::new(locale.translate("layout-new-folder"), true),
            ChildOf(menu),
            BookmarkMenuMessage(NewFolderInputRequest {
                webview,
                parent: None,
            }),
        ));
    }

    fn pin_menu(
        menu: Entity,
        commands: &mut Commands,
        locale: &Locale,
        entries: &BookmarkMenuEntries<'_, '_>,
        uuid: &str,
    ) {
        let Some((_, _, _, Some(metadata), true, bookmarked, _, _, _)) =
            entries.iter().find(|(_, id, ..)| id.0 == uuid)
        else {
            return;
        };
        commands.spawn((
            OsMenuEntry::new(locale.translate("common-open"), true),
            ChildOf(menu),
            BookmarkMenuMessage(OpenRequest {
                url: Some(metadata.url.clone()),
            }),
        ));
        commands.spawn((
            OsMenuEntry::new(locale.translate("layout-unpin-page"), true),
            ChildOf(menu),
            BookmarkMenuMessage(UnpinRequest {
                uuid: uuid.to_string(),
            }),
        ));
        if bookmarked {
            commands.spawn((OsMenuSeparator, ChildOf(menu)));
            commands.spawn((
                OsMenuEntry::new(locale.translate("layout-remove-bookmark"), true),
                ChildOf(menu),
                BookmarkMenuMessage(RemoveRequest {
                    uuid: uuid.to_string(),
                }),
            ));
        }
    }

    fn bookmark_menu(
        menu: Entity,
        commands: &mut Commands,
        locale: &Locale,
        entries: &BookmarkMenuEntries<'_, '_>,
        folders: &[FolderMenuRow],
        uuid: &str,
        webview: Entity,
    ) {
        let Some((_, _, _, Some(metadata), pinned, true, _, _, parent)) =
            entries.iter().find(|(_, id, ..)| id.0 == uuid)
        else {
            return;
        };
        commands.spawn((
            OsMenuEntry::new(locale.translate("common-open"), true),
            ChildOf(menu),
            BookmarkMenuMessage(OpenRequest {
                url: Some(metadata.url.clone()),
            }),
        ));
        commands.spawn((
            OsMenuEntry::new(locale.translate("common-rename"), true),
            ChildOf(menu),
            BookmarkMenuMessage(RenameInputRequest {
                webview,
                uuid: uuid.to_string(),
            }),
        ));
        if pinned {
            commands.spawn((
                OsMenuEntry::new(locale.translate("layout-unpin-page"), true),
                ChildOf(menu),
                BookmarkMenuMessage(UnpinRequest {
                    uuid: uuid.to_string(),
                }),
            ));
        } else {
            commands.spawn((
                OsMenuEntry::new(locale.translate("layout-pin"), true),
                ChildOf(menu),
                BookmarkMenuMessage(PinRequest {
                    uuid: uuid.to_string(),
                }),
            ));
        }
        commands.spawn((OsMenuSeparator, ChildOf(menu)));
        if parent.is_some() {
            commands.spawn((
                OsMenuEntry::new(locale.translate("layout-move-to-bookmarks"), true),
                ChildOf(menu),
                BookmarkMenuMessage(MoveRequest {
                    uuid: uuid.to_string(),
                    folder: None,
                }),
            ));
        }
        for folder in FolderChoice::collect(folders) {
            if parent.is_some_and(|parent| parent.parent() == folder.entity) {
                continue;
            }
            commands.spawn((
                OsMenuEntry::new(
                    locale.translate_with(
                        "layout-move-to",
                        &[("folder", TranslationValue::String(&folder.label))],
                    ),
                    true,
                ),
                ChildOf(menu),
                BookmarkMenuMessage(MoveRequest {
                    uuid: uuid.to_string(),
                    folder: Some(folder.uuid),
                }),
            ));
        }
        commands.spawn((OsMenuSeparator, ChildOf(menu)));
        commands.spawn((
            OsMenuEntry::new(locale.translate("common-remove"), true),
            ChildOf(menu),
            BookmarkMenuMessage(RemoveRequest {
                uuid: uuid.to_string(),
            }),
        ));
    }

    fn folder_menu(
        menu: Entity,
        commands: &mut Commands,
        locale: &Locale,
        entries: &BookmarkMenuEntries<'_, '_>,
        folders: &[FolderMenuRow],
        uuid: &str,
        active_page: Option<PageMetadata>,
        webview: Entity,
    ) {
        let Some((entity, _, _, _, _, _, true, collapsed, parent)) =
            entries.iter().find(|(_, id, ..)| id.0 == uuid)
        else {
            return;
        };
        commands.spawn((
            OsMenuEntry::new(
                locale.translate(if collapsed {
                    "common-expand"
                } else {
                    "common-collapse"
                }),
                true,
            ),
            ChildOf(menu),
            BookmarkMenuMessage(ToggleFolderRequest {
                uuid: uuid.to_string(),
            }),
        ));
        let current_page_enabled = active_page.is_some();
        let current_page = active_page.unwrap_or_default();
        commands.spawn((
            OsMenuEntry::new(
                locale.translate("layout-bookmark-current-page"),
                current_page_enabled,
            ),
            ChildOf(menu),
            BookmarkMenuMessage(AddRequest {
                metadata: current_page,
                folder: Some(uuid.to_string()),
            }),
        ));
        let new_folder_item = OsMenuEntry::new(locale.translate("layout-new-folder"), true);
        let new_folder = NewFolderInputRequest {
            webview,
            parent: Some(uuid.to_string()),
        };
        if collapsed {
            commands.spawn((
                new_folder_item,
                ChildOf(menu),
                BookmarkMenuMessage(new_folder),
                BookmarkMenuMessage(ToggleFolderRequest {
                    uuid: uuid.to_string(),
                }),
            ));
        } else {
            commands.spawn((
                new_folder_item,
                ChildOf(menu),
                BookmarkMenuMessage(new_folder),
            ));
        }
        commands.spawn((
            OsMenuEntry::new(locale.translate("layout-rename-folder"), true),
            ChildOf(menu),
            BookmarkMenuMessage(RenameInputRequest {
                webview,
                uuid: uuid.to_string(),
            }),
        ));
        commands.spawn((OsMenuSeparator, ChildOf(menu)));
        if parent.is_some() {
            commands.spawn((
                OsMenuEntry::new(locale.translate("layout-move-to-bookmarks"), true),
                ChildOf(menu),
                BookmarkMenuMessage(MoveFolderRequest {
                    uuid: uuid.to_string(),
                    parent: None,
                }),
            ));
        }
        for folder in FolderChoice::collect(folders) {
            if folder.entity == entity
                || FolderMenuRow::is_descendant(folder.entity, entity, folders)
            {
                continue;
            }
            commands.spawn((
                OsMenuEntry::new(
                    locale.translate_with(
                        "layout-move-to",
                        &[("folder", TranslationValue::String(&folder.label))],
                    ),
                    true,
                ),
                ChildOf(menu),
                BookmarkMenuMessage(MoveFolderRequest {
                    uuid: uuid.to_string(),
                    parent: Some(folder.uuid),
                }),
            ));
        }
        commands.spawn((OsMenuSeparator, ChildOf(menu)));
        commands.spawn((
            OsMenuEntry::new(locale.translate("layout-remove-folder"), true),
            ChildOf(menu),
            BookmarkMenuMessage(RemoveFolderRequest {
                uuid: uuid.to_string(),
            }),
        ));
    }

    impl FolderMenuRow {
        fn collect(entries: &BookmarkMenuEntries<'_, '_>) -> Vec<Self> {
            let mut folders = Vec::new();
            for (entity, uuid, name, _, _, _, folder, _, parent) in entries {
                if !folder {
                    continue;
                }
                folders.push(Self {
                    entity,
                    uuid: uuid.0.clone(),
                    name: name
                        .map(|name| name.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    parent: parent.map(Relationship::get),
                });
            }
            folders
        }

        fn is_descendant(candidate: Entity, ancestor: Entity, folders: &[Self]) -> bool {
            let mut current = Some(candidate);
            let mut visited = HashSet::new();
            while let Some(entity) = current {
                if !visited.insert(entity) {
                    return false;
                }
                let Some(folder) = folders.iter().find(|folder| folder.entity == entity) else {
                    return false;
                };
                let Some(parent) = folder.parent else {
                    return false;
                };
                if parent == ancestor {
                    return true;
                }
                current = Some(parent);
            }
            false
        }
    }

    struct FolderChoice {
        entity: Entity,
        uuid: String,
        label: String,
    }

    impl FolderChoice {
        fn collect(folders: &[FolderMenuRow]) -> Vec<Self> {
            let mut choices = Vec::new();
            Self::append(folders, None, "", &mut HashSet::new(), &mut choices);
            choices
        }

        fn append(
            folders: &[FolderMenuRow],
            parent: Option<Entity>,
            prefix: &str,
            visited: &mut HashSet<Entity>,
            choices: &mut Vec<Self>,
        ) {
            for folder in folders.iter().filter(|folder| folder.parent == parent) {
                if !visited.insert(folder.entity) {
                    continue;
                }
                let label = if prefix.is_empty() {
                    folder.name.clone()
                } else {
                    format!("{prefix} / {}", folder.name)
                };
                choices.push(Self {
                    entity: folder.entity,
                    uuid: folder.uuid.clone(),
                    label: label.clone(),
                });
                Self::append(folders, Some(folder.entity), &label, visited, choices);
            }
        }
    }

    fn forward_message<M: Message + Clone>(
        mut selections: MessageReader<OsMenuSelection>,
        menu_messages: Query<&BookmarkMenuMessage<M>>,
        mut messages: MessageWriter<M>,
    ) {
        for selection in selections.read() {
            let Ok(message) = menu_messages.get(selection.target()) else {
                continue;
            };
            messages.write(message.0.clone());
        }
    }

    fn spawn_input_revision(mut commands: Commands) {
        commands.spawn(BookmarkMenuInputRevision::default());
    }

    fn begin_input(
        mut new_folders: MessageReader<NewFolderInputRequest>,
        mut renames: MessageReader<RenameInputRequest>,
        mut revision: Single<&mut BookmarkMenuInputRevision>,
        mut commands: Commands,
    ) {
        for request in new_folders.read() {
            revision.0 = revision.0.wrapping_add(1);
            commands.trigger(UiStateWrite::<LayoutUiState>::from_event(
                request.webview,
                &BookmarkMenuEffect {
                    create_folder: BookmarkFolderCreateEffect {
                        revision: revision.0,
                        parent: request.parent.clone(),
                    },
                    ..Default::default()
                },
            ));
        }
        for request in renames.read() {
            revision.0 = revision.0.wrapping_add(1);
            commands.trigger(UiStateWrite::<LayoutUiState>::from_event(
                request.webview,
                &BookmarkMenuEffect {
                    rename: BookmarkRenameEffect {
                        revision: revision.0,
                        uuid: request.uuid.clone(),
                    },
                    ..Default::default()
                },
            ));
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use bevy::ecs::message::Messages;

        #[test]
        fn selected_menu_entity_forwards_each_typed_message() {
            let mut app = App::new();
            app.add_plugins(MinimalPlugins)
                .add_message::<OsMenuSelection>()
                .add_message::<PinRequest>()
                .add_message::<RemoveRequest>()
                .add_systems(
                    Update,
                    (
                        forward_message::<PinRequest>,
                        forward_message::<RemoveRequest>,
                    ),
                );

            let selected = app
                .world_mut()
                .spawn((
                    OsMenuEntry::new("selected".to_string(), true),
                    BookmarkMenuMessage(PinRequest {
                        uuid: "pin".to_string(),
                    }),
                    BookmarkMenuMessage(RemoveRequest {
                        uuid: "remove".to_string(),
                    }),
                ))
                .id();
            let disabled = app
                .world_mut()
                .spawn((
                    OsMenuEntry::new("disabled".to_string(), false),
                    BookmarkMenuMessage(PinRequest {
                        uuid: "disabled".to_string(),
                    }),
                ))
                .id();

            app.world_mut()
                .write_message(OsMenuSelection::new(selected));
            app.world_mut().run_schedule(Update);

            let pins = app
                .world_mut()
                .resource_mut::<Messages<PinRequest>>()
                .drain()
                .collect::<Vec<_>>();
            let removes = app
                .world_mut()
                .resource_mut::<Messages<RemoveRequest>>()
                .drain()
                .collect::<Vec<_>>();
            assert_eq!(pins, vec![PinRequest { uuid: "pin".into() }]);
            assert_eq!(
                removes,
                vec![RemoveRequest {
                    uuid: "remove".into()
                }]
            );
            assert!(app.world().get_entity(selected).is_ok());
            assert!(app.world().get_entity(disabled).is_ok());
        }
    }
}
