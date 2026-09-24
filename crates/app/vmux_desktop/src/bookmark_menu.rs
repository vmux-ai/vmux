use bevy::prelude::*;

impl Plugin for BookmarkMenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(vmux_layout::LayoutContractPlugin)
            .init_resource::<vmux_layout::window::FocusedWindow>();
        #[cfg(target_os = "macos")]
        app.add_message::<macos::NewFolderInputRequest>()
            .add_message::<macos::RenameInputRequest>()
            .init_resource::<macos::BookmarkMenuState>()
            .init_resource::<macos::BookmarkMenuInputSequence>()
            .add_systems(
                Update,
                (
                    macos::show_bookmark_menu,
                    macos::begin_new_folder_input,
                    macos::begin_rename_input,
                ),
            );
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
    use bevy::ecs::message::Messages;
    use bevy::ecs::relationship::Relationship;
    use bevy::ecs::system::NonSendMarker;
    use bevy::prelude::*;
    use muda::ContextMenu;
    use std::collections::{HashMap, HashSet};
    use vmux_api::bookmark::BookmarkMenuActionEvent;
    use vmux_core::{Bookmark, Collapsed, Folder, PageMetadata, Pin, Uuid};
    use vmux_layout::bookmark::{
        AddRequest, BookmarkMenuTarget, MoveFolderRequest, MoveRequest, PinRequest,
        RemoveFolderRequest, RemoveRequest, ShowBookmarkMenuRequest, ToggleFolderRequest,
        UnpinRequest,
    };
    use vmux_layout::stack::StackRequest;
    use vmux_ui::i18n::{Locale, TranslationValue};

    thread_local! {
        static HELD_MENU: std::cell::RefCell<Option<muda::Menu>> =
            const { std::cell::RefCell::new(None) };
    }

    #[derive(Resource, Default)]
    pub(super) struct BookmarkMenuState {
        next_id: u64,
        selections: HashMap<String, BookmarkMenuSelection>,
    }

    #[derive(Resource, Default)]
    pub(super) struct BookmarkMenuInputSequence(u64);

    impl BookmarkMenuInputSequence {
        fn send(
            &mut self,
            commands: &mut Commands,
            webview: Entity,
            action: &str,
            uuid: Option<String>,
        ) {
            self.0 = self.0.wrapping_add(1);
            vmux_layout::LayoutUiStateUpdates::write(
                commands,
                webview,
                &BookmarkMenuActionEvent {
                    sequence: self.0,
                    action: action.to_string(),
                    uuid,
                },
            );
        }
    }

    #[derive(Clone)]
    struct BookmarkMenuSelection {
        webview: Entity,
        choice: BookmarkMenuChoice,
    }

    #[derive(Clone)]
    enum BookmarkMenuChoice {
        Open(String),
        Add {
            metadata: PageMetadata,
            folder: Option<String>,
        },
        Move {
            uuid: String,
            folder: Option<String>,
        },
        MoveFolder {
            uuid: String,
            parent: Option<String>,
        },
        Pin(String),
        Remove(String),
        RemoveFolder(String),
        ToggleFolder(String),
        Unpin(String),
        BeginNewFolder {
            parent: Option<String>,
            expand: bool,
        },
        BeginRename(String),
    }

    #[derive(Clone)]
    struct FolderMenuRow {
        entity: Entity,
        uuid: String,
        name: String,
        parent: Option<Entity>,
    }

    #[derive(Message)]
    pub(super) struct NewFolderInputRequest {
        webview: Entity,
        parent: Option<String>,
    }

    #[derive(Message)]
    pub(super) struct RenameInputRequest {
        webview: Entity,
        uuid: String,
    }

    struct BookmarkMenuBuilder<'a> {
        menu: muda::Menu,
        webview: Entity,
        menu_id: u64,
        item_index: usize,
        selections: &'a mut HashMap<String, BookmarkMenuSelection>,
    }

    impl<'a> BookmarkMenuBuilder<'a> {
        fn new(
            webview: Entity,
            menu_id: u64,
            selections: &'a mut HashMap<String, BookmarkMenuSelection>,
        ) -> Self {
            Self {
                menu: muda::Menu::new(),
                webview,
                menu_id,
                item_index: 0,
                selections,
            }
        }

        fn item(&mut self, label: String, enabled: bool, choice: BookmarkMenuChoice) {
            let id = format!("bookmark_context_{}_{}", self.menu_id, self.item_index);
            self.item_index += 1;
            let item = muda::MenuItem::with_id(id.clone(), label, enabled, None);
            if self.menu.append(&item).is_ok() {
                self.selections.insert(
                    id,
                    BookmarkMenuSelection {
                        webview: self.webview,
                        choice,
                    },
                );
            }
        }

        fn separator(&mut self) {
            let _ = self.menu.append(&muda::PredefinedMenuItem::separator());
        }

        fn show(self, view_ptr: *mut std::ffi::c_void) {
            HELD_MENU.with(|held| {
                *held.borrow_mut() = Some(self.menu);
                if let Some(menu) = held.borrow().as_ref() {
                    unsafe {
                        menu.show_context_menu_for_nsview(view_ptr as _, None);
                    }
                }
            });
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn show_bookmark_menu(
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
        mut state: ResMut<BookmarkMenuState>,
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

        state.selections.clear();
        let menu_id = state.next_id;
        state.next_id = state.next_id.wrapping_add(1);
        let locale = Locale::requested(Some(&settings.appearance.locale));
        let folders = folder_rows(&entries);
        let mut builder = BookmarkMenuBuilder::new(request.webview, menu_id, &mut state.selections);
        match request.target {
            BookmarkMenuTarget::Root => root_menu(&mut builder, &locale),
            BookmarkMenuTarget::Pin { uuid } => pin_menu(&mut builder, &locale, &entries, &uuid),
            BookmarkMenuTarget::Entry { uuid } => {
                bookmark_menu(&mut builder, &locale, &entries, &folders, &uuid)
            }
            BookmarkMenuTarget::Folder { uuid, active_page } => folder_menu(
                &mut builder,
                &locale,
                &entries,
                &folders,
                &uuid,
                active_page,
            ),
        }
        builder.show(view_ptr);
    }

    fn root_menu(builder: &mut BookmarkMenuBuilder, locale: &Locale) {
        builder.item(
            locale.translate("layout-new-folder"),
            true,
            BookmarkMenuChoice::BeginNewFolder {
                parent: None,
                expand: false,
            },
        );
    }

    fn pin_menu(
        builder: &mut BookmarkMenuBuilder,
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
        builder.item(
            locale.translate("common-open"),
            true,
            BookmarkMenuChoice::Open(metadata.url.clone()),
        );
        builder.item(
            locale.translate("layout-unpin-page"),
            true,
            BookmarkMenuChoice::Unpin(uuid.to_string()),
        );
        if bookmarked {
            builder.separator();
            builder.item(
                locale.translate("layout-remove-bookmark"),
                true,
                BookmarkMenuChoice::Remove(uuid.to_string()),
            );
        }
    }

    fn bookmark_menu(
        builder: &mut BookmarkMenuBuilder,
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
    ) {
        let Some((_, _, _, Some(metadata), pinned, true, _, _, parent)) =
            entries.iter().find(|(_, id, ..)| id.0 == uuid)
        else {
            return;
        };
        builder.item(
            locale.translate("common-open"),
            true,
            BookmarkMenuChoice::Open(metadata.url.clone()),
        );
        builder.item(
            locale.translate("common-rename"),
            true,
            BookmarkMenuChoice::BeginRename(uuid.to_string()),
        );
        builder.item(
            locale.translate(if pinned {
                "layout-unpin-page"
            } else {
                "layout-pin"
            }),
            true,
            if pinned {
                BookmarkMenuChoice::Unpin(uuid.to_string())
            } else {
                BookmarkMenuChoice::Pin(uuid.to_string())
            },
        );
        builder.separator();
        if parent.is_some() {
            builder.item(
                locale.translate("layout-move-to-bookmarks"),
                true,
                BookmarkMenuChoice::Move {
                    uuid: uuid.to_string(),
                    folder: None,
                },
            );
        }
        for folder in folder_choices(folders) {
            if parent.is_some_and(|parent| parent.parent() == folder.entity) {
                continue;
            }
            builder.item(
                locale.translate_with(
                    "layout-move-to",
                    &[("folder", TranslationValue::String(&folder.label))],
                ),
                true,
                BookmarkMenuChoice::Move {
                    uuid: uuid.to_string(),
                    folder: Some(folder.uuid),
                },
            );
        }
        builder.separator();
        builder.item(
            locale.translate("common-remove"),
            true,
            BookmarkMenuChoice::Remove(uuid.to_string()),
        );
    }

    fn folder_menu(
        builder: &mut BookmarkMenuBuilder,
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
    ) {
        let Some((entity, _, _, _, _, _, true, collapsed, parent)) =
            entries.iter().find(|(_, id, ..)| id.0 == uuid)
        else {
            return;
        };
        builder.item(
            locale.translate(if collapsed {
                "common-expand"
            } else {
                "common-collapse"
            }),
            true,
            BookmarkMenuChoice::ToggleFolder(uuid.to_string()),
        );
        let current_page_enabled = active_page.is_some();
        let current_page = active_page.unwrap_or_default();
        builder.item(
            locale.translate("layout-bookmark-current-page"),
            current_page_enabled,
            BookmarkMenuChoice::Add {
                metadata: current_page,
                folder: Some(uuid.to_string()),
            },
        );
        builder.item(
            locale.translate("layout-new-folder"),
            true,
            BookmarkMenuChoice::BeginNewFolder {
                parent: Some(uuid.to_string()),
                expand: collapsed,
            },
        );
        builder.item(
            locale.translate("layout-rename-folder"),
            true,
            BookmarkMenuChoice::BeginRename(uuid.to_string()),
        );
        builder.separator();
        if parent.is_some() {
            builder.item(
                locale.translate("layout-move-to-bookmarks"),
                true,
                BookmarkMenuChoice::MoveFolder {
                    uuid: uuid.to_string(),
                    parent: None,
                },
            );
        }
        for folder in folder_choices(folders) {
            if folder.entity == entity || folder_is_descendant(folder.entity, entity, folders) {
                continue;
            }
            builder.item(
                locale.translate_with(
                    "layout-move-to",
                    &[("folder", TranslationValue::String(&folder.label))],
                ),
                true,
                BookmarkMenuChoice::MoveFolder {
                    uuid: uuid.to_string(),
                    parent: Some(folder.uuid),
                },
            );
        }
        builder.separator();
        builder.item(
            locale.translate("layout-remove-folder"),
            true,
            BookmarkMenuChoice::RemoveFolder(uuid.to_string()),
        );
    }

    fn folder_rows(
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
    ) -> Vec<FolderMenuRow> {
        let mut folders = Vec::new();
        for (entity, uuid, name, _, _, _, folder, _, parent) in entries {
            if !folder {
                continue;
            }
            folders.push(FolderMenuRow {
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

    struct FolderChoice {
        entity: Entity,
        uuid: String,
        label: String,
    }

    fn folder_choices(folders: &[FolderMenuRow]) -> Vec<FolderChoice> {
        fn append(
            folders: &[FolderMenuRow],
            parent: Option<Entity>,
            prefix: &str,
            visited: &mut HashSet<Entity>,
            choices: &mut Vec<FolderChoice>,
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
                choices.push(FolderChoice {
                    entity: folder.entity,
                    uuid: folder.uuid.clone(),
                    label: label.clone(),
                });
                append(folders, Some(folder.entity), &label, visited, choices);
            }
        }

        let mut choices = Vec::new();
        append(folders, None, "", &mut HashSet::new(), &mut choices);
        choices
    }

    fn folder_is_descendant(
        candidate: Entity,
        ancestor: Entity,
        folders: &[FolderMenuRow],
    ) -> bool {
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

    pub(super) fn forward_menu_event(world: &mut World, event_id: &str) -> bool {
        let Some(selection) = world
            .resource_mut::<BookmarkMenuState>()
            .selections
            .remove(event_id)
        else {
            return false;
        };
        match selection.choice {
            BookmarkMenuChoice::Open(url) => {
                world
                    .resource_mut::<Messages<StackRequest>>()
                    .write(StackRequest::Open { url: Some(url) });
            }
            BookmarkMenuChoice::Add { metadata, folder } => {
                world
                    .resource_mut::<Messages<AddRequest>>()
                    .write(AddRequest { metadata, folder });
            }
            BookmarkMenuChoice::Move { uuid, folder } => {
                world
                    .resource_mut::<Messages<MoveRequest>>()
                    .write(MoveRequest { uuid, folder });
            }
            BookmarkMenuChoice::MoveFolder { uuid, parent } => {
                world
                    .resource_mut::<Messages<MoveFolderRequest>>()
                    .write(MoveFolderRequest { uuid, parent });
            }
            BookmarkMenuChoice::Pin(uuid) => {
                world
                    .resource_mut::<Messages<PinRequest>>()
                    .write(PinRequest { uuid });
            }
            BookmarkMenuChoice::Remove(uuid) => {
                world
                    .resource_mut::<Messages<RemoveRequest>>()
                    .write(RemoveRequest { uuid });
            }
            BookmarkMenuChoice::RemoveFolder(uuid) => {
                world
                    .resource_mut::<Messages<RemoveFolderRequest>>()
                    .write(RemoveFolderRequest { uuid });
            }
            BookmarkMenuChoice::ToggleFolder(uuid) => {
                world
                    .resource_mut::<Messages<ToggleFolderRequest>>()
                    .write(ToggleFolderRequest { uuid });
            }
            BookmarkMenuChoice::Unpin(uuid) => {
                world
                    .resource_mut::<Messages<UnpinRequest>>()
                    .write(UnpinRequest { uuid });
            }
            BookmarkMenuChoice::BeginNewFolder { parent, expand } => {
                if expand && let Some(uuid) = &parent {
                    world
                        .resource_mut::<Messages<ToggleFolderRequest>>()
                        .write(ToggleFolderRequest { uuid: uuid.clone() });
                }
                world
                    .resource_mut::<Messages<NewFolderInputRequest>>()
                    .write(NewFolderInputRequest {
                        webview: selection.webview,
                        parent,
                    });
            }
            BookmarkMenuChoice::BeginRename(uuid) => {
                world
                    .resource_mut::<Messages<RenameInputRequest>>()
                    .write(RenameInputRequest {
                        webview: selection.webview,
                        uuid,
                    });
            }
        }
        true
    }

    pub(super) fn begin_new_folder_input(
        mut reader: MessageReader<NewFolderInputRequest>,
        mut sequence: ResMut<BookmarkMenuInputSequence>,
        mut commands: Commands,
    ) {
        for request in reader.read() {
            sequence.send(
                &mut commands,
                request.webview,
                "new_folder",
                request.parent.clone(),
            );
        }
    }

    pub(super) fn begin_rename_input(
        mut reader: MessageReader<RenameInputRequest>,
        mut sequence: ResMut<BookmarkMenuInputSequence>,
        mut commands: Commands,
    ) {
        for request in reader.read() {
            sequence.send(
                &mut commands,
                request.webview,
                "rename",
                Some(request.uuid.clone()),
            );
        }
    }
}
