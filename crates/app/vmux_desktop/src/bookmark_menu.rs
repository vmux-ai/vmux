use bevy::prelude::*;

impl Plugin for BookmarkMenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(vmux_layout::LayoutContractPlugin)
            .init_resource::<vmux_layout::window::FocusedWindow>();
        #[cfg(target_os = "macos")]
        app.add_message::<macos::BookmarkMenuSelection>()
            .init_resource::<macos::BookmarkMenuActionSequence>()
            .add_systems(
                Update,
                (
                    macos::show_bookmark_menu,
                    macos::apply_bookmark_menu_selection,
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
    use parking_lot::Mutex;
    use std::collections::{HashMap, HashSet};
    use std::sync::LazyLock;
    use std::sync::atomic::{AtomicU64, Ordering};
    use vmux_api::bookmark::BookmarkMenuActionEvent;
    use vmux_core::{Bookmark, Collapsed, Folder, PageMetadata, Pin, Uuid};
    use vmux_layout::bookmark::{BookmarkMenuTarget, BookmarkMutation, ShowBookmarkMenuRequest};
    use vmux_ui::i18n::{Locale, TranslationValue};

    thread_local! {
        static HELD_MENU: std::cell::RefCell<Option<muda::Menu>> =
            const { std::cell::RefCell::new(None) };
    }

    static NEXT_MENU: AtomicU64 = AtomicU64::new(0);
    static PENDING_ACTIONS: LazyLock<Mutex<HashMap<String, BookmarkMenuSelection>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));

    #[derive(Resource, Default)]
    pub(super) struct BookmarkMenuActionSequence(u64);

    impl BookmarkMenuActionSequence {
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

    #[derive(Message, Clone)]
    pub(super) struct BookmarkMenuSelection {
        webview: Entity,
        action: BookmarkMenuAction,
    }

    #[derive(Clone)]
    enum BookmarkMenuAction {
        Open(String),
        Apply(BookmarkMutation),
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

    struct BookmarkMenuBuilder {
        menu: muda::Menu,
        webview: Entity,
        menu_id: u64,
        item_index: usize,
    }

    impl BookmarkMenuBuilder {
        fn new(webview: Entity) -> Self {
            Self {
                menu: muda::Menu::new(),
                webview,
                menu_id: NEXT_MENU.fetch_add(1, Ordering::Relaxed),
                item_index: 0,
            }
        }

        fn item(&mut self, label: String, enabled: bool, action: BookmarkMenuAction) {
            let id = format!("bookmark_context_{}_{}", self.menu_id, self.item_index);
            self.item_index += 1;
            let item = muda::MenuItem::with_id(id.clone(), label, enabled, None);
            if self.menu.append(&item).is_ok() {
                PENDING_ACTIONS.lock().insert(
                    id,
                    BookmarkMenuSelection {
                        webview: self.webview,
                        action,
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

        PENDING_ACTIONS.lock().clear();
        let locale = Locale::requested(Some(&settings.appearance.locale));
        let folders = folder_rows(&entries);
        let mut builder = BookmarkMenuBuilder::new(request.webview);
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
            BookmarkMenuAction::BeginNewFolder {
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
            BookmarkMenuAction::Open(metadata.url.clone()),
        );
        builder.item(
            locale.translate("layout-unpin-page"),
            true,
            BookmarkMenuAction::Apply(BookmarkMutation::Unpin {
                uuid: uuid.to_string(),
            }),
        );
        if bookmarked {
            builder.separator();
            builder.item(
                locale.translate("layout-remove-bookmark"),
                true,
                BookmarkMenuAction::Apply(BookmarkMutation::Remove {
                    uuid: uuid.to_string(),
                }),
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
            BookmarkMenuAction::Open(metadata.url.clone()),
        );
        builder.item(
            locale.translate("common-rename"),
            true,
            BookmarkMenuAction::BeginRename(uuid.to_string()),
        );
        builder.item(
            locale.translate(if pinned {
                "layout-unpin-page"
            } else {
                "layout-pin"
            }),
            true,
            BookmarkMenuAction::Apply(if pinned {
                BookmarkMutation::Unpin {
                    uuid: uuid.to_string(),
                }
            } else {
                BookmarkMutation::Pin {
                    uuid: uuid.to_string(),
                }
            }),
        );
        builder.separator();
        if parent.is_some() {
            builder.item(
                locale.translate("layout-move-to-bookmarks"),
                true,
                BookmarkMenuAction::Apply(BookmarkMutation::Move {
                    uuid: uuid.to_string(),
                    folder: None,
                }),
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
                BookmarkMenuAction::Apply(BookmarkMutation::Move {
                    uuid: uuid.to_string(),
                    folder: Some(folder.uuid),
                }),
            );
        }
        builder.separator();
        builder.item(
            locale.translate("common-remove"),
            true,
            BookmarkMenuAction::Apply(BookmarkMutation::Remove {
                uuid: uuid.to_string(),
            }),
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
            BookmarkMenuAction::Apply(BookmarkMutation::ToggleFolder {
                uuid: uuid.to_string(),
            }),
        );
        let current_page_enabled = active_page.is_some();
        let current_page = active_page.unwrap_or_default();
        builder.item(
            locale.translate("layout-bookmark-current-page"),
            current_page_enabled,
            BookmarkMenuAction::Apply(BookmarkMutation::Add {
                metadata: current_page,
                folder: Some(uuid.to_string()),
            }),
        );
        builder.item(
            locale.translate("layout-new-folder"),
            true,
            BookmarkMenuAction::BeginNewFolder {
                parent: Some(uuid.to_string()),
                expand: collapsed,
            },
        );
        builder.item(
            locale.translate("layout-rename-folder"),
            true,
            BookmarkMenuAction::BeginRename(uuid.to_string()),
        );
        builder.separator();
        if parent.is_some() {
            builder.item(
                locale.translate("layout-move-to-bookmarks"),
                true,
                BookmarkMenuAction::Apply(BookmarkMutation::MoveFolder {
                    uuid: uuid.to_string(),
                    parent: None,
                }),
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
                BookmarkMenuAction::Apply(BookmarkMutation::MoveFolder {
                    uuid: uuid.to_string(),
                    parent: Some(folder.uuid),
                }),
            );
        }
        builder.separator();
        builder.item(
            locale.translate("layout-remove-folder"),
            true,
            BookmarkMenuAction::Apply(BookmarkMutation::RemoveFolder {
                uuid: uuid.to_string(),
            }),
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
        let Some(selection) = PENDING_ACTIONS.lock().remove(event_id) else {
            return false;
        };
        world
            .resource_mut::<Messages<BookmarkMenuSelection>>()
            .write(selection);
        true
    }

    pub(super) fn apply_bookmark_menu_selection(
        mut reader: MessageReader<BookmarkMenuSelection>,
        mut bookmark_mutations: MessageWriter<BookmarkMutation>,
        mut stack_requests: MessageWriter<vmux_layout::stack::StackRequest>,
        mut sequence: ResMut<BookmarkMenuActionSequence>,
        mut commands: Commands,
    ) {
        for selection in reader.read() {
            match &selection.action {
                BookmarkMenuAction::Open(url) => {
                    stack_requests.write(vmux_layout::stack::StackRequest::Open {
                        url: Some(url.clone()),
                    });
                }
                BookmarkMenuAction::Apply(operation) => {
                    bookmark_mutations.write(operation.clone());
                }
                BookmarkMenuAction::BeginNewFolder { parent, expand } => {
                    if *expand && let Some(uuid) = parent {
                        bookmark_mutations
                            .write(BookmarkMutation::ToggleFolder { uuid: uuid.clone() });
                    }
                    sequence.send(
                        &mut commands,
                        selection.webview,
                        "new_folder",
                        parent.clone(),
                    );
                }
                BookmarkMenuAction::BeginRename(uuid) => {
                    sequence.send(
                        &mut commands,
                        selection.webview,
                        "rename",
                        Some(uuid.clone()),
                    );
                }
            }
        }
    }
}
