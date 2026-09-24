use bevy::prelude::*;

#[cfg(not(target_os = "macos"))]
impl Plugin for BookmarkMenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(vmux_layout::LayoutContractPlugin)
            .init_resource::<vmux_layout::window::FocusedWindow>();
    }
}

pub(crate) struct BookmarkMenuPlugin;

pub(crate) fn forward_menu_event(world: &mut World, event_id: &str) -> bool {
    #[cfg(target_os = "macos")]
    {
        macos::forward_menu_event(world, event_id)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = world;
        let _ = event_id;
        false
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use bevy::ecs::relationship::Relationship;
    use bevy::ecs::system::NonSendMarker;
    use bevy::prelude::*;
    use muda::ContextMenu;
    use std::collections::HashSet;
    use vmux_api::bookmark::{BookmarkMenuEffect, BookmarkMenuInput};
    use vmux_core::{Bookmark, Collapsed, Folder, PageMetadata, Pin, Uuid, host::UiStateWrite};
    use vmux_layout::bookmark::{
        AddRequest, BookmarkMenuTarget, MoveFolderRequest, MoveRequest, PinRequest,
        RemoveFolderRequest, RemoveRequest, ShowBookmarkMenuRequest, ToggleFolderRequest,
        UnpinRequest,
    };
    use vmux_layout::stack::OpenRequest;
    use vmux_layout::state::LayoutUiState;
    use vmux_ui::i18n::{Locale, TranslationValue};

    impl Plugin for super::BookmarkMenuPlugin {
        fn build(&self, app: &mut App) {
            app.add_plugins(vmux_layout::LayoutContractPlugin)
                .init_resource::<vmux_layout::window::FocusedWindow>()
                .add_message::<NewFolderInputRequest>()
                .add_message::<RenameInputRequest>()
                .add_systems(Startup, spawn_bookmark_menu_input_revision)
                .add_observer(forward_message::<OpenRequest>)
                .add_observer(forward_message::<AddRequest>)
                .add_observer(forward_message::<MoveRequest>)
                .add_observer(forward_message::<MoveFolderRequest>)
                .add_observer(forward_message::<PinRequest>)
                .add_observer(forward_message::<RemoveRequest>)
                .add_observer(forward_message::<RemoveFolderRequest>)
                .add_observer(forward_message::<ToggleFolderRequest>)
                .add_observer(forward_message::<UnpinRequest>)
                .add_observer(forward_message::<NewFolderInputRequest>)
                .add_observer(forward_message::<RenameInputRequest>)
                .add_systems(Update, (show_bookmark_menu, present_bookmark_menu).chain())
                .add_systems(
                    Update,
                    begin_bookmark_menu_input.after(vmux_layout::bookmark::BookmarkRequestSet),
                );
        }
    }

    thread_local! {
        static HELD_MENU: std::cell::RefCell<Option<muda::Menu>> =
            const { std::cell::RefCell::new(None) };
        static PENDING_MENU: std::cell::RefCell<Option<(muda::Menu, *mut std::ffi::c_void)>> =
            const { std::cell::RefCell::new(None) };
    }

    #[derive(Component, Default)]
    struct BookmarkMenuInputRevision(u64);

    #[derive(Component)]
    struct BookmarkMenuItem {
        id: String,
        enabled: bool,
    }

    #[derive(Component)]
    struct BookmarkMenuMessage<M: Message>(M);

    #[derive(EntityEvent)]
    struct BookmarkMenuSelected(#[event_target] Entity);

    #[derive(Clone)]
    struct FolderMenuRow {
        entity: Entity,
        uuid: String,
        name: String,
        parent: Option<Entity>,
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

    struct BookmarkMenuBuilder {
        menu: muda::Menu,
        item_index: usize,
    }

    impl BookmarkMenuBuilder {
        fn new() -> Self {
            Self {
                menu: muda::Menu::new(),
                item_index: 0,
            }
        }

        fn item(&mut self, label: String, enabled: bool) -> BookmarkMenuItem {
            let id = format!("bookmark_context_{}", self.item_index);
            self.item_index += 1;
            let item = muda::MenuItem::with_id(id.clone(), label, enabled, None);
            let _ = self.menu.append(&item);
            BookmarkMenuItem { id, enabled }
        }

        fn separator(&mut self) {
            let _ = self.menu.append(&muda::PredefinedMenuItem::separator());
        }

        fn queue(self, view_ptr: *mut std::ffi::c_void) {
            PENDING_MENU.with(|pending| {
                *pending.borrow_mut() = Some((self.menu, view_ptr));
            });
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn show_bookmark_menu(
        _non_send: NonSendMarker,
        mut reader: MessageReader<ShowBookmarkMenuRequest>,
        focused: Res<vmux_layout::window::FocusedWindow>,
        entries: Query<(
            Entity,
            &Uuid,
            Option<&Name>,
            Option<&PageMetadata>,
            Has<Pin>,
            Has<Bookmark>,
            Has<Folder>,
            Has<Collapsed>,
            Option<&ChildOf>,
        )>,
        settings: Res<vmux_setting::AppSettings>,
        menu_items: Query<Entity, With<BookmarkMenuItem>>,
        mut commands: Commands,
    ) {
        use bevy::winit::WINIT_WINDOWS;
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};

        let Some(request) = reader.read().last().cloned() else {
            return;
        };

        let Some(window_entity) = focused.0 else {
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

        for entity in &menu_items {
            commands.entity(entity).despawn();
        }
        let locale = Locale::requested(Some(&settings.appearance.locale));
        let folders = FolderMenuRow::collect(&entries);
        let mut builder = BookmarkMenuBuilder::new();
        match request.target {
            BookmarkMenuTarget::Root => {
                root_menu(&mut builder, &mut commands, &locale, request.webview)
            }
            BookmarkMenuTarget::Pin { uuid } => {
                pin_menu(&mut builder, &mut commands, &locale, &entries, &uuid)
            }
            BookmarkMenuTarget::Entry { uuid } => bookmark_menu(
                &mut builder,
                &mut commands,
                &locale,
                &entries,
                &folders,
                &uuid,
                request.webview,
            ),
            BookmarkMenuTarget::Folder { uuid, active_page } => folder_menu(
                &mut builder,
                &mut commands,
                &locale,
                &entries,
                &folders,
                &uuid,
                active_page,
                request.webview,
            ),
        }
        builder.queue(view_ptr);
    }

    fn present_bookmark_menu(_non_send: NonSendMarker) {
        PENDING_MENU.with(|pending| {
            let Some((menu, view_ptr)) = pending.borrow_mut().take() else {
                return;
            };
            HELD_MENU.with(|held| {
                *held.borrow_mut() = Some(menu);
                if let Some(menu) = held.borrow().as_ref() {
                    unsafe {
                        menu.show_context_menu_for_nsview(view_ptr as _, None);
                    }
                }
            });
        });
    }

    fn root_menu(
        builder: &mut BookmarkMenuBuilder,
        commands: &mut Commands,
        locale: &Locale,
        webview: Entity,
    ) {
        commands.spawn((
            builder.item(locale.translate("layout-new-folder"), true),
            BookmarkMenuMessage(NewFolderInputRequest {
                webview,
                parent: None,
            }),
        ));
    }

    fn pin_menu(
        builder: &mut BookmarkMenuBuilder,
        commands: &mut Commands,
        locale: &Locale,
        entries: &Query<(
            Entity,
            &Uuid,
            Option<&Name>,
            Option<&PageMetadata>,
            Has<Pin>,
            Has<Bookmark>,
            Has<Folder>,
            Has<Collapsed>,
            Option<&ChildOf>,
        )>,
        uuid: &str,
    ) {
        let Some((_, _, _, Some(metadata), true, bookmarked, _, _, _)) =
            entries.iter().find(|(_, id, ..)| id.0 == uuid)
        else {
            return;
        };
        commands.spawn((
            builder.item(locale.translate("common-open"), true),
            BookmarkMenuMessage(OpenRequest {
                url: Some(metadata.url.clone()),
            }),
        ));
        commands.spawn((
            builder.item(locale.translate("layout-unpin-page"), true),
            BookmarkMenuMessage(UnpinRequest {
                uuid: uuid.to_string(),
            }),
        ));
        if bookmarked {
            builder.separator();
            commands.spawn((
                builder.item(locale.translate("layout-remove-bookmark"), true),
                BookmarkMenuMessage(RemoveRequest {
                    uuid: uuid.to_string(),
                }),
            ));
        }
    }

    fn bookmark_menu(
        builder: &mut BookmarkMenuBuilder,
        commands: &mut Commands,
        locale: &Locale,
        entries: &Query<(
            Entity,
            &Uuid,
            Option<&Name>,
            Option<&PageMetadata>,
            Has<Pin>,
            Has<Bookmark>,
            Has<Folder>,
            Has<Collapsed>,
            Option<&ChildOf>,
        )>,
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
            builder.item(locale.translate("common-open"), true),
            BookmarkMenuMessage(OpenRequest {
                url: Some(metadata.url.clone()),
            }),
        ));
        commands.spawn((
            builder.item(locale.translate("common-rename"), true),
            BookmarkMenuMessage(RenameInputRequest {
                webview,
                uuid: uuid.to_string(),
            }),
        ));
        if pinned {
            commands.spawn((
                builder.item(locale.translate("layout-unpin-page"), true),
                BookmarkMenuMessage(UnpinRequest {
                    uuid: uuid.to_string(),
                }),
            ));
        } else {
            commands.spawn((
                builder.item(locale.translate("layout-pin"), true),
                BookmarkMenuMessage(PinRequest {
                    uuid: uuid.to_string(),
                }),
            ));
        }
        builder.separator();
        if parent.is_some() {
            commands.spawn((
                builder.item(locale.translate("layout-move-to-bookmarks"), true),
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
                builder.item(
                    locale.translate_with(
                        "layout-move-to",
                        &[("folder", TranslationValue::String(&folder.label))],
                    ),
                    true,
                ),
                BookmarkMenuMessage(MoveRequest {
                    uuid: uuid.to_string(),
                    folder: Some(folder.uuid),
                }),
            ));
        }
        builder.separator();
        commands.spawn((
            builder.item(locale.translate("common-remove"), true),
            BookmarkMenuMessage(RemoveRequest {
                uuid: uuid.to_string(),
            }),
        ));
    }

    fn folder_menu(
        builder: &mut BookmarkMenuBuilder,
        commands: &mut Commands,
        locale: &Locale,
        entries: &Query<(
            Entity,
            &Uuid,
            Option<&Name>,
            Option<&PageMetadata>,
            Has<Pin>,
            Has<Bookmark>,
            Has<Folder>,
            Has<Collapsed>,
            Option<&ChildOf>,
        )>,
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
            builder.item(
                locale.translate(if collapsed {
                    "common-expand"
                } else {
                    "common-collapse"
                }),
                true,
            ),
            BookmarkMenuMessage(ToggleFolderRequest {
                uuid: uuid.to_string(),
            }),
        ));
        let current_page_enabled = active_page.is_some();
        let current_page = active_page.unwrap_or_default();
        commands.spawn((
            builder.item(
                locale.translate("layout-bookmark-current-page"),
                current_page_enabled,
            ),
            BookmarkMenuMessage(AddRequest {
                metadata: current_page,
                folder: Some(uuid.to_string()),
            }),
        ));
        let new_folder_item = builder.item(locale.translate("layout-new-folder"), true);
        let new_folder = NewFolderInputRequest {
            webview,
            parent: Some(uuid.to_string()),
        };
        if collapsed {
            commands.spawn((
                new_folder_item,
                BookmarkMenuMessage(new_folder),
                BookmarkMenuMessage(ToggleFolderRequest {
                    uuid: uuid.to_string(),
                }),
            ));
        } else {
            commands.spawn((new_folder_item, BookmarkMenuMessage(new_folder)));
        }
        commands.spawn((
            builder.item(locale.translate("layout-rename-folder"), true),
            BookmarkMenuMessage(RenameInputRequest {
                webview,
                uuid: uuid.to_string(),
            }),
        ));
        builder.separator();
        if parent.is_some() {
            commands.spawn((
                builder.item(locale.translate("layout-move-to-bookmarks"), true),
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
                builder.item(
                    locale.translate_with(
                        "layout-move-to",
                        &[("folder", TranslationValue::String(&folder.label))],
                    ),
                    true,
                ),
                BookmarkMenuMessage(MoveFolderRequest {
                    uuid: uuid.to_string(),
                    parent: Some(folder.uuid),
                }),
            ));
        }
        builder.separator();
        commands.spawn((
            builder.item(locale.translate("layout-remove-folder"), true),
            BookmarkMenuMessage(RemoveFolderRequest {
                uuid: uuid.to_string(),
            }),
        ));
    }

    impl FolderMenuRow {
        fn collect(
            entries: &Query<(
                Entity,
                &Uuid,
                Option<&Name>,
                Option<&PageMetadata>,
                Has<Pin>,
                Has<Bookmark>,
                Has<Folder>,
                Has<Collapsed>,
                Option<&ChildOf>,
            )>,
        ) -> Vec<Self> {
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

    pub(super) fn forward_menu_event(world: &mut World, event_id: &str) -> bool {
        let selected = {
            let mut items = world.query::<(Entity, &BookmarkMenuItem)>();
            items
                .iter(world)
                .find_map(|(entity, item)| (item.enabled && item.id == event_id).then_some(entity))
        };
        let Some(entity) = selected else {
            return false;
        };
        world.trigger(BookmarkMenuSelected(entity));
        world.despawn(entity);
        true
    }

    fn forward_message<M: Message + Clone>(
        trigger: On<BookmarkMenuSelected>,
        menu_messages: Query<&BookmarkMenuMessage<M>>,
        mut messages: MessageWriter<M>,
    ) {
        let Ok(message) = menu_messages.get(trigger.event_target()) else {
            return;
        };
        messages.write(message.0.clone());
    }

    fn spawn_bookmark_menu_input_revision(mut commands: Commands) {
        commands.spawn(BookmarkMenuInputRevision::default());
    }

    fn begin_bookmark_menu_input(
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
                    revision: revision.0,
                    input: Some(BookmarkMenuInput::CreateFolder {
                        parent: request.parent.clone(),
                    }),
                },
            ));
        }
        for request in renames.read() {
            revision.0 = revision.0.wrapping_add(1);
            commands.trigger(UiStateWrite::<LayoutUiState>::from_event(
                request.webview,
                &BookmarkMenuEffect {
                    revision: revision.0,
                    input: Some(BookmarkMenuInput::Rename {
                        uuid: request.uuid.clone(),
                    }),
                },
            ));
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[derive(Message, Clone)]
        struct FirstRequest(u32);

        #[derive(Message, Clone)]
        struct SecondRequest(&'static str);

        #[derive(Resource, Default)]
        struct Received {
            first: Vec<u32>,
            second: Vec<&'static str>,
        }

        fn receive(
            mut first: MessageReader<FirstRequest>,
            mut second: MessageReader<SecondRequest>,
            mut received: ResMut<Received>,
        ) {
            for request in first.read() {
                received.first.push(request.0);
            }
            for request in second.read() {
                received.second.push(request.0);
            }
        }

        #[test]
        fn selected_menu_entity_forwards_each_typed_message() {
            let mut app = App::new();
            app.add_plugins(MinimalPlugins)
                .add_message::<FirstRequest>()
                .add_message::<SecondRequest>()
                .add_observer(forward_message::<FirstRequest>)
                .add_observer(forward_message::<SecondRequest>)
                .init_resource::<Received>()
                .add_systems(Update, receive);

            let selected = app
                .world_mut()
                .spawn((
                    BookmarkMenuItem {
                        id: "selected".to_string(),
                        enabled: true,
                    },
                    BookmarkMenuMessage(FirstRequest(7)),
                    BookmarkMenuMessage(SecondRequest("second")),
                ))
                .id();
            let disabled = app
                .world_mut()
                .spawn((
                    BookmarkMenuItem {
                        id: "disabled".to_string(),
                        enabled: false,
                    },
                    BookmarkMenuMessage(FirstRequest(9)),
                ))
                .id();

            assert!(forward_menu_event(app.world_mut(), "selected"));
            assert!(!forward_menu_event(app.world_mut(), "disabled"));
            app.update();

            let received = app.world().resource::<Received>();
            assert_eq!(received.first, vec![7]);
            assert_eq!(received.second, vec!["second"]);
            assert!(app.world().get_entity(selected).is_err());
            assert!(app.world().get_entity(disabled).is_ok());
        }
    }
}
