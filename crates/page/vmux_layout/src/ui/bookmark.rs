use std::rc::Rc;

use dioxus::html::input_data::MouseButton;
use dioxus::prelude::*;
use vmux_api::bookmark::{
    BookmarkAddRequest, BookmarkContextMenuRequest, BookmarkFolderChoice,
    BookmarkFolderCreateRequest, BookmarkFolderMoveRequest, BookmarkFolderRemoveRequest,
    BookmarkFolderRenameRequest, BookmarkFolderRow, BookmarkFolderToggleRequest,
    BookmarkMenuEffect, BookmarkMenuEntryRequest, BookmarkMenuFolderRequest, BookmarkMenuInput,
    BookmarkMenuPinRequest, BookmarkMenuRootRequest, BookmarkMovePinRequest, BookmarkMoveRequest,
    BookmarkNode, BookmarkOpenRequest, BookmarkPinRequest, BookmarkPinUrlRequest,
    BookmarkRemoveRequest, BookmarkRenameRequest, BookmarkReorderPinRequest, BookmarkRow,
    BookmarkStateEvent, BookmarkTextInputRequest, BookmarkUnpinRequest,
};
use vmux_core::PageMetadata;
use vmux_ui::components::context_menu::{ContextMenu, ContextMenuContent, ContextMenuItem};
use vmux_ui::components::icon::Icon;
use vmux_ui::components::inline_edit::InlineEdit;
use vmux_ui::components::tree_row::{
    SIDEBAR_CARD_CHEVRON_CLOSED, SIDEBAR_CARD_CHEVRON_OPEN, SIDEBAR_TREE_COLUMN,
    SIDEBAR_TREE_SCROLLER, SidebarTreeChildren, SidebarTreeRow, SidebarTreeRowGroup,
};
use vmux_ui::favicon::Favicon;
use vmux_ui::hooks::send;
use vmux_ui::i18n::{TranslationValue, translate, translate_with};
use vmux_ui::icon::PageIconView;
use vmux_ui::platform::sleep_ms;

use crate::event::{SideSheetSectionRequest, StackNode};

#[component]
pub(super) fn BookmarksSection(
    bookmarks: BookmarkStateEvent,
    active_page: Option<StackNode>,
    pane_id: u64,
    expanded: bool,
) -> Element {
    let BookmarkStateEvent {
        pins,
        roots,
        folders,
    } = bookmarks;
    let drag_state: Signal<Option<BookmarkDragState>> = use_context();
    let mut optimistic_pin_order: Signal<Option<OptimisticPinOrder>> = use_context();
    let mut creating_folder = use_signal(|| false);
    let new_folder_draft = use_signal(|| translate("layout-new-folder"));
    let bookmark_menu: Memo<BookmarkMenuEffect> = use_context();
    let initial_menu_revision = bookmark_menu.peek().revision;
    let mut handled_menu_revision = use_signal(|| initial_menu_revision);
    use_effect(move || {
        let effect = bookmark_menu();
        if effect.revision == handled_menu_revision() {
            return;
        }
        handled_menu_revision.set(effect.revision);
        if matches!(
            effect.input,
            Some(BookmarkMenuInput::CreateFolder { parent: None })
        ) {
            begin_new_folder(creating_folder, new_folder_draft);
        }
    });
    let folder_rows = bookmark_folder_rows(&roots);
    let host_pin_order = pins.iter().map(|pin| pin.uuid.clone()).collect::<Vec<_>>();
    let observed_host_pin_order = host_pin_order.clone();
    use_effect(use_reactive!(|observed_host_pin_order| {
        let Some(optimistic) = optimistic_pin_order() else {
            return;
        };
        if observed_host_pin_order == optimistic.expected
            || observed_host_pin_order != optimistic.baseline
        {
            optimistic_pin_order.set(None);
        }
    }));
    let pins = optimistic_pin_order()
        .as_ref()
        .map(|optimistic| optimistic.apply(&pins))
        .unwrap_or(pins);
    let active_url = active_page.as_ref().map(|page| page.url.clone());
    let rendered_pin_order = Rc::new(pins.iter().map(|pin| pin.uuid.clone()).collect::<Vec<_>>());
    let root_targeted = bookmark_drop_targeted(drag_state, &BookmarkDropTarget::Root);
    let root_drop_label = drag_state()
        .filter(|drag| drag.active)
        .map(|drag| match drag.item {
            BookmarkDragItem::Page { .. } => translate("layout-add-to-bookmarks"),
            BookmarkDragItem::Bookmark { .. }
            | BookmarkDragItem::Pin { .. }
            | BookmarkDragItem::Folder { .. } => translate("layout-move-to-bookmarks"),
        });
    let bookmarks_title = translate("layout-bookmarks");
    let new_folder_title = translate("layout-new-folder");

    rsx! {
        div {
            "data-bookmark-drop": "root",
            class: "glass group relative z-30 mb-2 flex shrink-0 flex-col overflow-hidden rounded-lg",
            oncontextmenu: move |e: Event<MouseData>| {
                e.prevent_default();
                BookmarkContextTarget::Root.request();
            },
            div {
                "data-bookmark-drop": "root",
                onpointerenter: move |_| set_bookmark_drop_target(drag_state, BookmarkDropTarget::Root),
                onpointerleave: move |_| clear_bookmark_drop_target(drag_state, &BookmarkDropTarget::Root),
                class: if root_targeted {
                    "flex items-center bg-foreground/10 ring-1 ring-inset ring-ring"
                } else {
                    "flex items-center transition-colors hover:bg-glass-hover"
                },
                div { class: "flex min-w-0 flex-1 items-center gap-2 px-2.5 py-2",
                    div { class: "grid h-7 w-7 shrink-0 place-items-center rounded-lg bg-foreground/[0.07] text-foreground ring-1 ring-inset ring-foreground/10",
                        Icon { class: "h-3.5 w-3.5",
                            path { d: "M19 21l-7-5-7 5V5a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2z" }
                        }
                    }
                    if let Some(label) = root_drop_label {
                        span { class: "min-w-0 flex-1 text-ui font-semibold text-foreground", "{label}" }
                    } else {
                        span { class: "min-w-0 flex-1 text-ui font-semibold text-foreground", "{bookmarks_title}" }
                        button {
                            r#type: "button",
                            aria_label: "{new_folder_title}",
                            title: "{new_folder_title}",
                            class: "flex h-6 w-6 shrink-0 cursor-pointer items-center justify-center rounded-sm text-muted-foreground hover:bg-foreground/10 hover:text-foreground",
                            onclick: move |event| {
                                event.prevent_default();
                                event.stop_propagation();
                                begin_new_folder(creating_folder, new_folder_draft);
                            },
                            Icon { class: "h-3.5 w-3.5 pointer-events-none",
                                path { d: "M4 20h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13c0 1.1.9 2 2 2Z" }
                                path { d: "M12 10v6" }
                                path { d: "M9 13h6" }
                            }
                        }
                    }
                }
                button {
                    r#type: "button",
                    aria_label: "{bookmarks_title}",
                    title: "{bookmarks_title}",
                    class: if expanded {
                        "mr-2 flex h-6 w-6 shrink-0 cursor-pointer items-center justify-center rounded-sm text-muted-foreground opacity-0 transition-opacity group-hover:opacity-100 focus-visible:opacity-100 hover:bg-foreground/10 hover:text-foreground"
                    } else {
                        "mr-2 flex h-6 w-6 shrink-0 cursor-pointer items-center justify-center rounded-sm bg-foreground/10 text-foreground"
                    },
                    onclick: move |_| {
                        let _ = send(&SideSheetSectionRequest::new(
                            pane_id,
                            "bookmarks",
                            !expanded,
                        ));
                    },
                    Icon {
                        class: if expanded { SIDEBAR_CARD_CHEVRON_OPEN } else { SIDEBAR_CARD_CHEVRON_CLOSED },
                        path { d: "m9 18 6-6-6-6" }
                    }
                }
            }
            div { class: if expanded {
                    "grid grid-rows-[1fr] opacity-100 transition-[grid-template-rows,opacity] duration-200 ease-out"
                } else {
                    "grid grid-rows-[0fr] opacity-0 transition-[grid-template-rows,opacity] duration-200 ease-out"
                },
                div { class: "overflow-hidden",
                    div { class: "border-t border-foreground/10 p-1.5",
                        if !pins.is_empty() {
                            div {
                                "data-bookmark-drop": "",
                                class: "mb-1 grid grid-cols-4 gap-1.5 p-1",
                                for (index, pin) in pins.iter().enumerate() {
                                    PinTile {
                                        key: "{pin.uuid}",
                                        row: pin.clone(),
                                        index,
                                        pin_order: rendered_pin_order.clone(),
                                        active: active_url.as_ref().is_some_and(|active_url| {
                                            let Some(active) = vmux_api::VmuxRoute::parse(active_url) else {
                                                return false;
                                            };
                                            vmux_api::VmuxRoute::parse(&pin.metadata.url)
                                                .is_some_and(|pin| active.same_page(&pin))
                                        }),
                                    }
                                }
                            }
                        }
                        if creating_folder() {
                            div { class: "flex h-9 items-center gap-2 rounded-md border border-transparent px-2",
                                Icon { class: "h-4 w-4 shrink-0 text-muted-foreground",
                                    path { d: "M4 20h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13c0 1.1.9 2 2 2Z" }
                                }
                                InlineEdit {
                                    draft: new_folder_draft,
                                    class: "min-w-0 flex-1 bg-transparent text-ui font-medium text-foreground outline-none".to_string(),
                                    placeholder: translate("layout-folder-name"),
                                    aria_label: translate("layout-folder-name"),
                                    on_active_change: set_bookmark_text_input_active,
                                    on_commit: move |name| {
                                        creating_folder.set(false);
                                        create_bookmark_folder(name, None);
                                    },
                                    on_cancel: move |_| creating_folder.set(false),
                                }
                            }
                        }
                        if pins.is_empty() && roots.is_empty() && !creating_folder() {
                            div { class: "px-2 py-2 text-ui-xs text-muted-foreground", {translate("layout-no-pins-bookmarks")} }
                        } else {
                            div { class: SIDEBAR_TREE_SCROLLER,
                            div { class: "{SIDEBAR_TREE_COLUMN} gap-1",
                                for node in roots.iter() {
                                    match node {
                                        BookmarkNode::Folder(f) if f.parent.is_none() => rsx! {
                                            BookmarkFolder {
                                                key: "{f.uuid}",
                                                folder: f.clone(),
                                                parent_uuid: None,
                                                folders: folders.clone(),
                                                folder_rows: folder_rows.clone(),
                                                active_page: active_page.clone(),
                                            }
                                        },
                                        BookmarkNode::Folder(_) => rsx! {},
                                        BookmarkNode::Entry(b) => rsx! {
                                            BookmarkEntry {
                                                key: "{b.uuid}",
                                                row: b.clone(),
                                                folder_uuid: None,
                                                folders: folders.clone(),
                                            }
                                        },
                                    }
                                }
                            }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Clone, PartialEq)]
pub(super) enum BookmarkDragItem {
    Page {
        metadata: PageMetadata,
    },
    Bookmark {
        uuid: String,
    },
    Pin {
        uuid: String,
        source_index: usize,
        order: Rc<Vec<String>>,
    },
    Folder {
        uuid: String,
    },
}

#[derive(Clone, PartialEq)]
enum BookmarkDropTarget {
    Root,
    Folder(String),
    Pin { uuid: String, index: usize },
}

#[derive(Clone, PartialEq)]
pub(super) struct BookmarkDragState {
    item: BookmarkDragItem,
    start_x: f64,
    start_y: f64,
    current_x: f64,
    current_y: f64,
    active: bool,
    target: Option<BookmarkDropTarget>,
}

#[derive(Clone, PartialEq)]
pub(super) struct OptimisticPinOrder {
    baseline: Vec<String>,
    expected: Vec<String>,
}

impl OptimisticPinOrder {
    fn after_drop(order: &[String], source_index: usize, target_index: usize) -> Option<Self> {
        if source_index == target_index
            || source_index >= order.len()
            || target_index >= order.len()
        {
            return None;
        }
        let mut expected = order.to_vec();
        let moved = expected.remove(source_index);
        expected.insert(target_index, moved);
        Some(Self {
            baseline: order.to_vec(),
            expected,
        })
    }

    fn apply(&self, pins: &[BookmarkRow]) -> Vec<BookmarkRow> {
        let mut ordered = Vec::with_capacity(pins.len());
        for uuid in &self.expected {
            if let Some(pin) = pins.iter().find(|pin| &pin.uuid == uuid) {
                ordered.push(pin.clone());
            }
        }
        for pin in pins {
            if !self.expected.contains(&pin.uuid) {
                ordered.push(pin.clone());
            }
        }
        ordered
    }
}

#[derive(Clone, Copy, Default, PartialEq)]
struct PinDragVisual {
    offset_x: f64,
    offset_y: f64,
    source: bool,
    active: bool,
    dragging: bool,
}

impl PinDragVisual {
    fn resolve(state: Option<&BookmarkDragState>, uuid: &str, index: usize) -> Self {
        let Some(state) = state.filter(|state| state.active) else {
            return Self::default();
        };
        let BookmarkDragItem::Pin {
            uuid: source_uuid,
            source_index,
            ..
        } = &state.item
        else {
            return Self::default();
        };
        let target_index = match state.target.as_ref() {
            Some(BookmarkDropTarget::Pin { index, .. }) => Some(*index),
            _ => None,
        };
        if source_uuid == uuid {
            return Self {
                offset_x: state.current_x - state.start_x,
                offset_y: state.current_y - state.start_y,
                source: true,
                active: true,
                dragging: true,
            };
        }
        let Some(target_index) = target_index else {
            return Self {
                dragging: true,
                ..Self::default()
            };
        };
        let destination =
            if *source_index < target_index && index > *source_index && index <= target_index {
                index - 1
            } else if target_index < *source_index && index >= target_index && index < *source_index
            {
                index + 1
            } else {
                index
            };
        let column_delta = destination as isize % 4 - index as isize % 4;
        let row_delta = destination as isize / 4 - index as isize / 4;
        Self {
            offset_x: column_delta as f64,
            offset_y: row_delta as f64,
            source: false,
            active: destination != index,
            dragging: true,
        }
    }

    fn style(self) -> String {
        if !self.dragging {
            return "transform:none;z-index:auto;pointer-events:auto;transition:transform 140ms ease;"
                .to_string();
        }
        if !self.active {
            return "transform:none;z-index:auto;pointer-events:none;transition:transform 140ms ease;"
                .to_string();
        }
        if self.source {
            return format!(
                "transform:translate3d({}px,{}px,0);z-index:20;pointer-events:none;transition:none;opacity:.9;",
                self.offset_x, self.offset_y
            );
        }
        format!(
            "transform:translate3d({}, {}, 0);z-index:auto;pointer-events:none;transition:transform 140ms ease;",
            pin_grid_offset(self.offset_x as isize),
            pin_grid_offset(self.offset_y as isize)
        )
    }
}

fn pin_grid_offset(delta: isize) -> String {
    match delta.cmp(&0) {
        std::cmp::Ordering::Equal => "0px".to_string(),
        std::cmp::Ordering::Greater => {
            format!("calc({}% + {}rem)", delta * 100, delta as f64 * 0.375)
        }
        std::cmp::Ordering::Less => format!(
            "calc({}% - {}rem)",
            delta * 100,
            delta.unsigned_abs() as f64 * 0.375
        ),
    }
}

impl BookmarkDragState {
    pub(super) fn begin(
        state: Signal<Option<Self>>,
        event: &Event<PointerData>,
        item: BookmarkDragItem,
    ) {
        begin_bookmark_drag(state, event, item);
    }

    pub(super) fn blocks_click(state: Signal<Option<Self>>) -> bool {
        bookmark_drag_blocks_click(state)
    }

    pub(super) fn listeners(
        state: Signal<Option<Self>>,
        optimistic_pin_order: Signal<Option<OptimisticPinOrder>>,
    ) -> Vec<Attribute> {
        if state.read().is_none() {
            return Vec::new();
        }

        vec![
            dioxus_elements::events::onpointermove(move |event| {
                update_bookmark_drag(state, &event)
            }),
            dioxus_elements::events::onpointerup(move |event| {
                end_bookmark_drag(state, optimistic_pin_order, &event)
            }),
            dioxus_elements::events::onpointercancel(move |event| {
                cancel_bookmark_drag(state, &event)
            }),
        ]
    }
}

pub(super) struct BookmarkContext;

impl BookmarkContext {
    pub(super) fn set_active(active: bool) {
        set_bookmark_context_menu_active(active);
    }
}

pub(super) struct BookmarkInput;

impl BookmarkInput {
    pub(super) fn set_active(active: bool) {
        set_bookmark_text_input_active(active);
    }

    pub(super) fn begin_rename(editing: Signal<bool>, draft: Signal<String>, name: String) {
        begin_inline_rename(editing, draft, name);
    }
}

fn bookmark_folder_rows(nodes: &[BookmarkNode]) -> Vec<BookmarkFolderRow> {
    nodes
        .iter()
        .filter_map(|node| match node {
            BookmarkNode::Folder(folder) => Some(folder.clone()),
            BookmarkNode::Entry(_) => None,
        })
        .collect()
}

#[component]
fn PinTile(row: BookmarkRow, index: usize, pin_order: Rc<Vec<String>>, active: bool) -> Element {
    let drag_state: Signal<Option<BookmarkDragState>> = use_context();
    let url_open = row.metadata.url.clone();
    let uuid_unpin = row.uuid.clone();
    let menu_val = use_signal(|| row.uuid.clone());
    let drop_target = BookmarkDropTarget::Pin {
        uuid: row.uuid.clone(),
        index,
    };
    let leave_target = drop_target.clone();
    let targeted = bookmark_drop_targeted(drag_state, &drop_target);
    let visual = PinDragVisual::resolve(drag_state().as_ref(), &row.uuid, index);
    let drag_item = BookmarkDragItem::Pin {
        uuid: row.uuid.clone(),
        source_index: index,
        order: pin_order,
    };
    rsx! {
        div {
            class: "relative aspect-square",
            onpointerenter: move |_| set_bookmark_drop_target(drag_state, drop_target.clone()),
            onpointerleave: move |_| clear_bookmark_drop_target(drag_state, &leave_target),
            if targeted {
                div { class: "pointer-events-none absolute inset-0 z-10 rounded-md ring-2 ring-primary/70" }
            }
            BookmarkContextMenu {
                target: BookmarkContextTarget::Pin { uuid: row.uuid.clone() },
                trigger: rsx! {
                    div {
                        "data-bookmark-drag-source": "true",
                        onpointerdown: {
                            let item = drag_item.clone();
                            move |event| begin_bookmark_drag(drag_state, &event, item.clone())
                        },
                        style: "{visual.style()}",
                        class: if visual.dragging {
                            "absolute inset-0 flex cursor-grabbing select-none items-center justify-center rounded-md bg-white/5"
                        } else if active {
                            "absolute inset-0 flex cursor-pointer select-none items-center justify-center rounded-md bg-primary/15 text-primary ring-1 ring-inset ring-primary/30 transition-colors hover:bg-primary/20"
                        } else {
                            "absolute inset-0 flex cursor-pointer select-none items-center justify-center rounded-md bg-white/5 transition-colors hover:bg-white/10"
                        },
                        onclick: {
                            let u = url_open.clone();
                            move |event| {
                                if bookmark_drag_blocks_click(drag_state) {
                                    event.prevent_default();
                                    event.stop_propagation();
                                    return;
                                }
                                open_bookmark(u.clone());
                            }
                        },
                        title: "{row.metadata.title}",
                        if vmux_api::VmuxRoute::parse(&row.metadata.url).is_some() {
                            Favicon {
                                favicon_url: row.metadata.icon.favicon_url().to_string(),
                                url: row.metadata.url.clone(),
                                class: "pointer-events-none h-5 w-5 shrink-0 rounded-sm object-contain".to_string(),
                                globe_class: "pointer-events-none h-5 w-5 shrink-0 text-muted-foreground".to_string(),
                            }
                        } else {
                            PageIconView {
                                icon: row.metadata.icon.clone(),
                                url: row.metadata.url.clone(),
                                img_class: "pointer-events-none h-5 w-5 shrink-0 rounded-sm object-contain".to_string(),
                                icon_class: "pointer-events-none h-5 w-5 shrink-0 text-muted-foreground".to_string(),
                            }
                        }
                    }
                },
                menu: rsx! {
                    SideSheetContextMenuContent {
                        ContextMenuItem {
                            index: 0usize,
                            value: Into::<ReadSignal<String>>::into(menu_val),
                            on_select: { let u = url_open.clone(); move |_: String| open_bookmark(u.clone()) },
                            attributes: vec![],
                            {translate("common-open")}
                        }
                        ContextMenuItem {
                            index: 1usize,
                            value: Into::<ReadSignal<String>>::into(menu_val),
                            on_select: { let id = uuid_unpin.clone(); move |_: String| BookmarkIdCommand::Unpin.send(id.clone()) },
                            attributes: vec![],
                            {translate("layout-unpin-page")}
                        }
                        if row.bookmarked {
                            ContextMenuItem {
                                index: 2usize,
                                value: Into::<ReadSignal<String>>::into(menu_val),
                                on_select: { let id = row.uuid.clone(); move |_: String| BookmarkIdCommand::Remove.send(id.clone()) },
                                attributes: vec![],
                                {translate("layout-remove-bookmark")}
                            }
                        }
                    }
                },
            }
        }
    }
}

fn open_bookmark(url: String) {
    let _ = send(&BookmarkOpenRequest { url });
}

#[derive(Clone, Copy)]
pub(super) enum BookmarkIdCommand {
    Remove,
    Pin,
    Unpin,
    ToggleFolder,
    RemoveFolder,
}

impl BookmarkIdCommand {
    pub(super) fn send(self, uuid: String) {
        match self {
            Self::Remove => {
                let _ = send(&BookmarkRemoveRequest { uuid });
            }
            Self::Pin => {
                let _ = send(&BookmarkPinRequest { uuid });
            }
            Self::Unpin => {
                let _ = send(&BookmarkUnpinRequest { uuid });
            }
            Self::ToggleFolder => {
                let _ = send(&BookmarkFolderToggleRequest { uuid });
            }
            Self::RemoveFolder => {
                let _ = send(&BookmarkFolderRemoveRequest { uuid });
            }
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum BookmarkPageCommand {
    Add,
    Pin,
}

impl BookmarkPageCommand {
    pub(super) fn send(self, metadata: PageMetadata, folder: Option<String>) {
        match self {
            Self::Add => {
                let _ = send(&BookmarkAddRequest { metadata, folder });
            }
            Self::Pin => {
                let _ = send(&BookmarkPinUrlRequest { metadata });
            }
        }
    }
}

fn move_bookmark(uuid: String, folder: Option<String>) {
    let _ = send(&BookmarkMoveRequest { uuid, folder });
}

fn move_pin(uuid: String, folder: Option<String>) {
    let _ = send(&BookmarkMovePinRequest { uuid, folder });
}

fn reorder_pin(uuid: String, target_uuid: String) -> bool {
    send(&BookmarkReorderPinRequest { uuid, target_uuid }).is_ok()
}

fn move_bookmark_folder(uuid: String, folder: Option<String>) {
    let _ = send(&BookmarkFolderMoveRequest {
        uuid,
        parent: folder,
    });
}

fn commit_bookmark_rename(uuid: String, name: String) {
    let name = name.trim().to_string();
    if name.is_empty() {
        return;
    }
    let _ = send(&BookmarkRenameRequest { uuid, name });
}

fn create_bookmark_folder(name: String, parent: Option<String>) {
    let name = name.trim().to_string();
    if name.is_empty() {
        return;
    }
    let _ = send(&BookmarkFolderCreateRequest { name, parent });
}

fn begin_bookmark_drag(
    mut state: Signal<Option<BookmarkDragState>>,
    event: &Event<PointerData>,
    item: BookmarkDragItem,
) {
    if event.trigger_button() != Some(MouseButton::Primary) {
        return;
    }
    event.prevent_default();
    let coordinates = event.client_coordinates();
    let target = match &item {
        BookmarkDragItem::Pin {
            uuid, source_index, ..
        } => Some(BookmarkDropTarget::Pin {
            uuid: uuid.clone(),
            index: *source_index,
        }),
        _ => None,
    };
    state.set(Some(BookmarkDragState {
        item,
        start_x: coordinates.x,
        start_y: coordinates.y,
        current_x: coordinates.x,
        current_y: coordinates.y,
        active: false,
        target,
    }));
}

fn update_bookmark_drag(mut state: Signal<Option<BookmarkDragState>>, event: &Event<PointerData>) {
    let Some(mut drag) = state() else {
        return;
    };
    let coordinates = event.client_coordinates();
    let dx = coordinates.x - drag.start_x;
    let dy = coordinates.y - drag.start_y;
    if !drag.active && dx * dx + dy * dy < 16.0 {
        return;
    }
    if !drag.active {
        set_bookmark_context_menu_active(true);
    }
    drag.current_x = coordinates.x;
    drag.current_y = coordinates.y;
    drag.active = true;
    state.set(Some(drag));
}

fn perform_bookmark_drop(
    item: BookmarkDragItem,
    target: BookmarkDropTarget,
    mut optimistic_pin_order: Signal<Option<OptimisticPinOrder>>,
) {
    match (item, target) {
        (
            BookmarkDragItem::Pin {
                uuid,
                source_index,
                order,
            },
            BookmarkDropTarget::Pin {
                uuid: target_uuid,
                index: target_index,
            },
        ) => {
            if uuid != target_uuid {
                let optimistic = OptimisticPinOrder::after_drop(&order, source_index, target_index);
                if !reorder_pin(uuid, target_uuid) {
                    optimistic_pin_order.set(None);
                    return;
                }
                optimistic_pin_order.set(optimistic.clone());
                spawn(async move {
                    sleep_ms(1_000).await;
                    if optimistic_pin_order() == optimistic {
                        optimistic_pin_order.set(None);
                    }
                });
            }
        }
        (BookmarkDragItem::Page { metadata }, BookmarkDropTarget::Root) => {
            BookmarkPageCommand::Add.send(metadata, None)
        }
        (BookmarkDragItem::Page { metadata }, BookmarkDropTarget::Folder(folder)) => {
            BookmarkPageCommand::Add.send(metadata, Some(folder))
        }
        (BookmarkDragItem::Bookmark { uuid }, BookmarkDropTarget::Root) => {
            move_bookmark(uuid, None)
        }
        (BookmarkDragItem::Bookmark { uuid }, BookmarkDropTarget::Folder(folder)) => {
            move_bookmark(uuid, Some(folder))
        }
        (BookmarkDragItem::Pin { uuid, .. }, BookmarkDropTarget::Root) => move_pin(uuid, None),
        (BookmarkDragItem::Pin { uuid, .. }, BookmarkDropTarget::Folder(folder)) => {
            move_pin(uuid, Some(folder))
        }
        (BookmarkDragItem::Folder { uuid }, BookmarkDropTarget::Root) => {
            move_bookmark_folder(uuid, None)
        }
        (BookmarkDragItem::Folder { uuid }, BookmarkDropTarget::Folder(folder)) => {
            if uuid != folder {
                move_bookmark_folder(uuid, Some(folder));
            }
        }
        (BookmarkDragItem::Page { .. }, BookmarkDropTarget::Pin { .. })
        | (BookmarkDragItem::Bookmark { .. }, BookmarkDropTarget::Pin { .. })
        | (BookmarkDragItem::Folder { .. }, BookmarkDropTarget::Pin { .. }) => {}
    }
}

fn clear_bookmark_drag_after_click(mut state: Signal<Option<BookmarkDragState>>) {
    spawn(async move {
        sleep_ms(0).await;
        state.set(None);
    });
}

fn end_bookmark_drag(
    mut state: Signal<Option<BookmarkDragState>>,
    optimistic_pin_order: Signal<Option<OptimisticPinOrder>>,
    event: &Event<PointerData>,
) {
    let Some(mut drag) = state() else {
        return;
    };
    let coordinates = event.client_coordinates();
    let dx = coordinates.x - drag.start_x;
    let dy = coordinates.y - drag.start_y;
    if !drag.active && dx * dx + dy * dy < 16.0 {
        state.set(None);
        return;
    }
    event.prevent_default();
    event.stop_propagation();
    drag.active = true;
    set_bookmark_context_menu_active(false);
    if let Some(target) = drag.target.clone() {
        perform_bookmark_drop(drag.item.clone(), target, optimistic_pin_order);
    }
    state.set(Some(drag));
    clear_bookmark_drag_after_click(state);
}

fn cancel_bookmark_drag(mut state: Signal<Option<BookmarkDragState>>, event: &Event<PointerData>) {
    event.prevent_default();
    set_bookmark_context_menu_active(false);
    state.set(None);
}

fn set_bookmark_drop_target(
    mut state: Signal<Option<BookmarkDragState>>,
    target: BookmarkDropTarget,
) {
    let Some(mut drag) = state() else {
        return;
    };
    if drag.target.as_ref() == Some(&target) {
        return;
    }
    drag.target = Some(target);
    state.set(Some(drag));
}

fn clear_bookmark_drop_target(
    mut state: Signal<Option<BookmarkDragState>>,
    target: &BookmarkDropTarget,
) {
    let Some(mut drag) = state() else {
        return;
    };
    if drag.target.as_ref() != Some(target) {
        return;
    }
    drag.target = None;
    state.set(Some(drag));
}

fn bookmark_drag_blocks_click(state: Signal<Option<BookmarkDragState>>) -> bool {
    state().is_some_and(|drag| drag.active)
}

fn bookmark_drop_targeted(
    state: Signal<Option<BookmarkDragState>>,
    target: &BookmarkDropTarget,
) -> bool {
    state().is_some_and(|drag| drag.active && drag.target.as_ref() == Some(target))
}

fn set_bookmark_text_input_active(active: bool) {
    let _ = send(&BookmarkTextInputRequest { active });
}

fn set_bookmark_context_menu_active(active: bool) {
    let _ = send(&BookmarkContextMenuRequest { active });
}

fn begin_inline_rename(mut editing: Signal<bool>, mut draft: Signal<String>, name: String) {
    draft.set(name);
    spawn(async move {
        sleep_ms(0).await;
        editing.set(true);
    });
}

#[component]
fn BookmarkFolder(
    folder: BookmarkFolderRow,
    parent_uuid: Option<String>,
    folders: Vec<BookmarkFolderChoice>,
    folder_rows: Vec<BookmarkFolderRow>,
    active_page: Option<StackNode>,
) -> Element {
    let drag_state: Signal<Option<BookmarkDragState>> = use_context();
    let uuid = folder.uuid.clone();
    let collapsed = folder.collapsed;
    let mut editing = use_signal(|| false);
    let draft = use_signal(|| folder.name.clone());
    let mut creating_child = use_signal(|| false);
    let child_draft = use_signal(|| translate("layout-new-folder"));
    let menu_val = use_signal(|| folder.uuid.clone());
    let new_folder_uuid = uuid.clone();
    let bookmark_menu: Memo<BookmarkMenuEffect> = use_context();
    let initial_menu_revision = bookmark_menu.peek().revision;
    let mut handled_menu_revision = use_signal(|| initial_menu_revision);
    let menu_uuid = uuid.clone();
    let menu_name = folder.name.clone();
    use_effect(move || {
        let effect = bookmark_menu();
        if effect.revision == handled_menu_revision() {
            return;
        }
        handled_menu_revision.set(effect.revision);
        match &effect.input {
            Some(BookmarkMenuInput::CreateFolder { parent })
                if parent.as_deref() == Some(menu_uuid.as_str()) =>
            {
                begin_new_folder(creating_child, child_draft);
            }
            Some(BookmarkMenuInput::Rename { uuid }) if uuid == &menu_uuid => {
                begin_inline_rename(editing, draft, menu_name.clone());
            }
            _ => {}
        }
    });
    let mut move_targets = Vec::new();
    if parent_uuid.is_some() {
        move_targets.push((None, translate("layout-move-to-bookmarks")));
    }
    move_targets.extend(
        folders
            .iter()
            .filter(|target| target.uuid != folder.uuid && !target.ancestors.contains(&folder.uuid))
            .map(|target| {
                (
                    Some(target.uuid.clone()),
                    translate_with(
                        "layout-move-to",
                        &[("folder", TranslationValue::String(&target.label))],
                    ),
                )
            }),
    );
    let remove_index = 4 + move_targets.len();
    let drop_target = BookmarkDropTarget::Folder(uuid.clone());
    let leave_target = drop_target.clone();
    let folder_targeted = bookmark_drop_targeted(drag_state, &drop_target);
    let drag_item = BookmarkDragItem::Folder { uuid: uuid.clone() };
    let mut child_folders = 0usize;
    for child in folder_rows.iter() {
        if child.parent.as_deref() == Some(folder.uuid.as_str()) {
            child_folders += 1;
        }
    }
    let child_count = child_folders + folder.children.len();
    let folder_is_empty = child_count == 0;
    let active_metadata = active_page.clone().map(|page| PageMetadata {
        title: page.title,
        url: page.url,
        icon: page.icon,
        bg_color: page.bg_color,
    });

    rsx! {
        div {
            "data-bookmark-drop": "{uuid}",
            class: "flex flex-col",
            onpointerenter: move |_| set_bookmark_drop_target(drag_state, drop_target.clone()),
            onpointerleave: move |_| clear_bookmark_drop_target(drag_state, &leave_target),
            if editing() {
                div { class: "flex h-9 items-center gap-2 rounded-md border border-transparent px-2",
                    Icon {
                        class: if collapsed {
                            "h-4 w-4 shrink-0 rotate-0 text-muted-foreground transition-transform duration-200 ease-out"
                        } else {
                            "h-4 w-4 shrink-0 rotate-90 text-muted-foreground transition-transform duration-200 ease-out"
                        },
                        path { d: "m9 18 6-6-6-6" }
                    }
                    InlineEdit {
                        draft,
                        class: "min-w-0 flex-1 bg-transparent text-ui font-medium text-foreground outline-none".to_string(),
                        placeholder: translate("layout-folder-name"),
                        aria_label: translate("layout-folder-name"),
                        on_active_change: set_bookmark_text_input_active,
                        on_commit: {
                            let id = uuid.clone();
                            move |name| {
                                editing.set(false);
                                commit_folder_rename(id.clone(), name);
                            }
                        },
                        on_cancel: move |_| editing.set(false),
                    }
                }
            } else {
                BookmarkContextMenu {
                    target: BookmarkContextTarget::Folder {
                        uuid: folder.uuid.clone(),
                        active_page: active_metadata,
                    },
                    trigger: rsx! {
                        div {
                            "data-bookmark-drag-source": "true",
                            class: if folder_targeted { "rounded-md ring-2 ring-ring" } else { "rounded-md" },
                            onpointerdown: {
                                let item = drag_item.clone();
                                move |event| begin_bookmark_drag(drag_state, &event, item.clone())
                            },
                            SidebarTreeRowGroup {
                                SidebarTreeRow {
                                    path: uuid.clone(),
                                    label: folder.name.clone(),
                                    is_dir: true,
                                    expanded: !collapsed,
                                    emphasis: true,
                                    title: folder.name.clone(),
                                    on_activate: {
                                        let id = uuid.clone();
                                        move |()| {
                                            if bookmark_drag_blocks_click(drag_state) {
                                                return;
                                            }
                                            BookmarkIdCommand::ToggleFolder.send(id.clone());
                                        }
                                    },
                                    trailing: rsx! {
                                        if child_count > 0 {
                                            span { class: "shrink-0 text-[10px] tabular-nums text-muted-foreground/70",
                                                "{child_count}"
                                            }
                                        }
                                    },
                                }
                            }
                        }
                    },
                    menu: rsx! {
                        SideSheetContextMenuContent {
                        ContextMenuItem {
                            index: 0usize,
                            value: Into::<ReadSignal<String>>::into(menu_val),
                            on_select: { let id = uuid.clone(); move |_: String| BookmarkIdCommand::ToggleFolder.send(id.clone()) },
                            attributes: vec![],
                            {if collapsed { translate("common-expand") } else { translate("common-collapse") }}
                        }
                        ContextMenuItem {
                            index: 1usize,
                            value: Into::<ReadSignal<String>>::into(menu_val),
                            disabled: active_page.is_none(),
                            on_select: {
                                let id = uuid.clone();
                                let page = active_page.clone();
                                move |_: String| {
                                    if let Some(page) = page.clone() {
                                        BookmarkPageCommand::Add.send(
                                            PageMetadata {
                                                title: page.title,
                                                url: page.url,
                                                icon: page.icon,
                                                bg_color: page.bg_color,
                                            },
                                            Some(id.clone()),
                                        );
                                    }
                                }
                            },
                            attributes: vec![],
                            {translate("layout-bookmark-current-page")}
                        }
                        ContextMenuItem {
                            index: 2usize,
                            value: Into::<ReadSignal<String>>::into(menu_val),
                            on_select: move |_: String| {
                                if collapsed {
                                    BookmarkIdCommand::ToggleFolder.send(new_folder_uuid.clone());
                                }
                                begin_new_folder(creating_child, child_draft);
                            },
                            attributes: vec![],
                            {translate("layout-new-folder")}
                        }
                        ContextMenuItem {
                            index: 3usize,
                            value: Into::<ReadSignal<String>>::into(menu_val),
                            on_select: {
                                let name = folder.name.clone();
                                move |_: String| begin_inline_rename(editing, draft, name.clone())
                            },
                            attributes: vec![],
                            {translate("layout-rename-folder")}
                        }
                        for (index, (target_folder, label)) in move_targets.iter().enumerate() {
                            ContextMenuItem {
                                key: "{index}",
                                index: 4usize + index,
                                value: Into::<ReadSignal<String>>::into(menu_val),
                                on_select: {
                                    let id = uuid.clone();
                                    let folder = target_folder.clone();
                                    move |_: String| move_bookmark_folder(id.clone(), folder.clone())
                                },
                                attributes: vec![],
                                "{label}"
                            }
                        }
                        ContextMenuItem {
                            index: remove_index,
                            value: Into::<ReadSignal<String>>::into(menu_val),
                            on_select: { let id = uuid.clone(); move |_: String| BookmarkIdCommand::RemoveFolder.send(id.clone()) },
                            attributes: vec![],
                            {translate("layout-remove-folder")}
                        }
                        }
                    },
                }
            }
            if creating_child() {
                div { class: "ml-3 flex h-9 items-center gap-2 rounded-md border border-transparent px-2",
                    Icon { class: "h-4 w-4 shrink-0 text-muted-foreground",
                        path { d: "M4 20h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13c0 1.1.9 2 2 2Z" }
                    }
                    InlineEdit {
                        draft: child_draft,
                        class: "min-w-0 flex-1 bg-transparent text-ui font-medium text-foreground outline-none".to_string(),
                        placeholder: translate("layout-folder-name"),
                        aria_label: translate("layout-folder-name"),
                        on_active_change: set_bookmark_text_input_active,
                        on_commit: {
                            let parent = uuid.clone();
                            move |name| {
                                creating_child.set(false);
                                create_bookmark_folder(name, Some(parent.clone()));
                            }
                        },
                        on_cancel: move |_| creating_child.set(false),
                    }
                }
            }
            SidebarTreeChildren { expanded: !collapsed,
                div { class: "ml-3 flex flex-col gap-1",
                    for child_folder in folder_rows
                        .iter()
                        .filter(|child| child.parent.as_deref() == Some(folder.uuid.as_str()))
                    {
                        BookmarkFolder {
                            key: "{child_folder.uuid}",
                            folder: child_folder.clone(),
                            parent_uuid: Some(folder.uuid.clone()),
                            folders: folders.clone(),
                            folder_rows: folder_rows.clone(),
                            active_page: active_page.clone(),
                        }
                    }
                    for bookmark in folder.children.iter() {
                        BookmarkEntry {
                            key: "{bookmark.uuid}",
                            row: bookmark.clone(),
                            folder_uuid: Some(folder.uuid.clone()),
                            folders: folders.clone(),
                        }
                    }
                    if folder_is_empty {
                        div { class: "px-2 py-1.5 text-ui-xs text-muted-foreground", {translate("layout-empty-folder")} }
                    }
                }
            }
        }
    }
}

fn begin_new_folder(mut creating: Signal<bool>, mut draft: Signal<String>) {
    draft.set(translate("layout-new-folder"));
    creating.set(true);
}

#[component]
fn BookmarkEntry(
    row: BookmarkRow,
    folder_uuid: Option<String>,
    folders: Vec<BookmarkFolderChoice>,
) -> Element {
    let drag_state: Signal<Option<BookmarkDragState>> = use_context();
    let url_open = row.metadata.url.clone();
    let uuid_pin = row.uuid.clone();
    let uuid_remove = row.uuid.clone();
    let uuid_rename = row.uuid.clone();
    let menu_val = use_signal(|| row.uuid.clone());
    let title = if row.metadata.title.is_empty() {
        row.metadata.url.clone()
    } else {
        row.metadata.title.clone()
    };
    let mut editing = use_signal(|| false);
    let draft = use_signal(|| title.clone());
    let bookmark_menu: Memo<BookmarkMenuEffect> = use_context();
    let initial_menu_revision = bookmark_menu.peek().revision;
    let mut handled_menu_revision = use_signal(|| initial_menu_revision);
    let menu_uuid = row.uuid.clone();
    let menu_name = title.clone();
    use_effect(move || {
        let effect = bookmark_menu();
        if effect.revision == handled_menu_revision() {
            return;
        }
        handled_menu_revision.set(effect.revision);
        if let Some(BookmarkMenuInput::Rename { uuid }) = &effect.input
            && uuid == &menu_uuid
        {
            begin_inline_rename(editing, draft, menu_name.clone());
        }
    });
    let mut move_targets: Vec<(Option<String>, String)> = Vec::new();
    if folder_uuid.is_some() {
        move_targets.push((None, translate("layout-move-to-bookmarks")));
    }
    move_targets.extend(
        folders
            .iter()
            .filter(|folder| Some(folder.uuid.as_str()) != folder_uuid.as_deref())
            .map(|folder| {
                (
                    Some(folder.uuid.clone()),
                    translate_with(
                        "layout-move-to",
                        &[("folder", TranslationValue::String(&folder.label))],
                    ),
                )
            }),
    );
    let remove_index = 3 + move_targets.len();
    let drag_item = BookmarkDragItem::Bookmark {
        uuid: row.uuid.clone(),
    };
    rsx! {
        if editing() {
            div { class: "flex h-9 items-center gap-2 rounded-md border border-transparent px-2",
                PageIconView {
                    icon: row.metadata.icon.clone(),
                    url: row.metadata.url.clone(),
                    img_class: "h-4 w-4 shrink-0 rounded-sm object-contain".to_string(),
                    icon_class: "h-4 w-4 shrink-0 text-muted-foreground".to_string(),
                }
                InlineEdit {
                    draft,
                    class: "min-w-0 flex-1 bg-transparent text-ui text-foreground outline-none".to_string(),
                    placeholder: String::new(),
                    aria_label: translate("common-rename"),
                    on_active_change: set_bookmark_text_input_active,
                    on_commit: {
                        let id = uuid_rename.clone();
                        move |name| {
                            editing.set(false);
                            commit_bookmark_rename(id.clone(), name);
                        }
                    },
                    on_cancel: move |_| editing.set(false),
                }
            }
        } else {
            BookmarkContextMenu {
                target: BookmarkContextTarget::Entry { uuid: row.uuid.clone() },
                trigger: rsx! {
                    div {
                        "data-bookmark-drag-source": "true",
                        onpointerdown: {
                            let item = drag_item.clone();
                            move |event| begin_bookmark_drag(drag_state, &event, item.clone())
                        },
                        SidebarTreeRowGroup {
                            SidebarTreeRow {
                                path: row.metadata.url.clone(),
                                label: title.clone(),
                                is_dir: false,
                                title: title.clone(),
                                leading: rsx! {
                                    PageIconView {
                                        icon: row.metadata.icon.clone(),
                                        url: row.metadata.url.clone(),
                                        img_class: "h-3.5 w-3.5 shrink-0 rounded-sm object-contain".to_string(),
                                        icon_class: "h-3.5 w-3.5 shrink-0 text-muted-foreground".to_string(),
                                    }
                                },
                                on_activate: {
                                    let u = url_open.clone();
                                    move |()| {
                                        if bookmark_drag_blocks_click(drag_state) {
                                            return;
                                        }
                                        open_bookmark(u.clone());
                                    }
                                },
                            }
                        }
                    }
                },
                menu: rsx! {
                    SideSheetContextMenuContent {
                    ContextMenuItem {
                        index: 0usize,
                        value: Into::<ReadSignal<String>>::into(menu_val),
                        on_select: { let u = url_open.clone(); move |_: String| open_bookmark(u.clone()) },
                        attributes: vec![],
                        {translate("common-open")}
                    }
                    ContextMenuItem {
                        index: 1usize,
                        value: Into::<ReadSignal<String>>::into(menu_val),
                        on_select: {
                            let name = title.clone();
                            move |_: String| begin_inline_rename(editing, draft, name.clone())
                        },
                        attributes: vec![],
                        {translate("common-rename")}
                    }
                    ContextMenuItem {
                        index: 2usize,
                        value: Into::<ReadSignal<String>>::into(menu_val),
                        on_select: {
                            let id = uuid_pin.clone();
                            let command = if row.pinned {
                                BookmarkIdCommand::Unpin
                            } else {
                                BookmarkIdCommand::Pin
                            };
                            move |_: String| command.send(id.clone())
                        },
                        attributes: vec![],
                        {if row.pinned { translate("layout-unpin-page") } else { translate("layout-pin") }}
                    }
                    for (index, (target_folder, label)) in move_targets.iter().enumerate() {
                        ContextMenuItem {
                            key: "{index}",
                            index: 3usize + index,
                            value: Into::<ReadSignal<String>>::into(menu_val),
                            on_select: {
                                let id = row.uuid.clone();
                                let folder = target_folder.clone();
                                move |_: String| move_bookmark(id.clone(), folder.clone())
                            },
                            attributes: vec![],
                            "{label}"
                        }
                    }
                    ContextMenuItem {
                        index: remove_index,
                        value: Into::<ReadSignal<String>>::into(menu_val),
                        on_select: { let id = uuid_remove.clone(); move |_: String| BookmarkIdCommand::Remove.send(id.clone()) },
                        attributes: vec![],
                        {translate("common-remove")}
                    }
                    }
                },
            }
        }
    }
}

fn commit_folder_rename(uuid: String, name: String) {
    let name = name.trim().to_string();
    if name.is_empty() {
        let _ = send(&BookmarkFolderRemoveRequest { uuid });
    } else {
        let _ = send(&BookmarkFolderRenameRequest { uuid, name });
    }
}

#[component]
pub(super) fn LayoutContextMenu(children: Element) -> Element {
    rsx! {
        ContextMenu {
            attributes: vec![
                dioxus_elements::events::oncontextmenu(move |event: Event<MouseData>| {
                    event.stop_propagation();
                }),
            ],
            on_open_change: set_bookmark_context_menu_active,
            {children}
        }
    }
}

#[component]
pub(super) fn SideSheetContextMenuContent(children: Element) -> Element {
    rsx! {
        ContextMenuContent { attributes: vec![], {children} }
    }
}

#[component]
fn BookmarkContextMenu(target: BookmarkContextTarget, trigger: Element, menu: Element) -> Element {
    #[cfg(target_os = "macos")]
    {
        let _ = menu;
        return rsx! {
            div {
                role: "button",
                aria_haspopup: "menu",
                user_select: "none",
                oncontextmenu: move |event: Event<MouseData>| {
                    event.prevent_default();
                    event.stop_propagation();
                    target.request();
                },
                {trigger}
            }
        };
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = target;
        rsx! {
            LayoutContextMenu {
                ContextMenuTrigger { attributes: vec![], {trigger} }
                {menu}
            }
        }
    }
}

#[derive(Clone, PartialEq)]
enum BookmarkContextTarget {
    Root,
    Pin {
        uuid: String,
    },
    Entry {
        uuid: String,
    },
    Folder {
        uuid: String,
        active_page: Option<PageMetadata>,
    },
}

impl BookmarkContextTarget {
    fn request(&self) {
        match self {
            Self::Root => {
                let _ = send(&BookmarkMenuRootRequest);
            }
            Self::Pin { uuid } => {
                let _ = send(&BookmarkMenuPinRequest { uuid: uuid.clone() });
            }
            Self::Entry { uuid } => {
                let _ = send(&BookmarkMenuEntryRequest { uuid: uuid.clone() });
            }
            Self::Folder { uuid, active_page } => {
                let _ = send(&BookmarkMenuFolderRequest {
                    uuid: uuid.clone(),
                    active_page: active_page.clone(),
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optimistic_pin_order_moves_the_source_to_the_target_slot() {
        let order = vec!["a".into(), "b".into(), "c".into(), "d".into()];

        let optimistic = OptimisticPinOrder::after_drop(&order, 0, 2).unwrap();

        assert_eq!(optimistic.baseline, order);
        assert_eq!(optimistic.expected, ["b", "c", "a", "d"]);
    }

    #[test]
    fn optimistic_pin_order_ignores_a_drop_on_the_same_slot() {
        let order = vec!["a".into(), "b".into()];

        assert!(OptimisticPinOrder::after_drop(&order, 1, 1).is_none());
    }
}
