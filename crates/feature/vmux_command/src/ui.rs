use crate::prompt_media::{ChatPasteMedia, ChatPickFiles};
use crate::ui::composer::{CommandComposerMenus, ComposerChips, ComposerMenuView};
use crate::ui::media::PromptMedia;
use crate::ui::signals::{
    COMMAND_BAR_INPUT_ID, CommandBarField, Readline, TypedDigit, use_palette_signals,
};
use dioxus::prelude::*;
use vmux_api::command_bar::CommandBarPicker;
use vmux_api::command_bar::{
    CommandBarFocusEffect, CommandBarOpenEvent, CommandBarUiState, CommandBarUiStatePatch,
    CommandPaletteActivateRequest, CommandPaletteDraftRequest, CommandPaletteHistoryMoveRequest,
    CommandPaletteMediaActivateRequest, CommandPaletteMediaDismissRequest,
    CommandPaletteMediaHighlightRequest, CommandPaletteMediaMoveRequest,
    CommandPaletteMenuActivateRequest, CommandPaletteMenuDismissRequest,
    CommandPaletteMenuMoveRequest, CommandPaletteRemoveAttachmentRequest, CommandPaletteState,
    CommandPaletteSubmitRequest, PaletteGlyph, PaletteMode,
};
use vmux_core::input::{UiKeyContext, Unclaimed};
use vmux_ui::agent_accent::agent_accent;
use vmux_ui::caret::{EventSelection, byte_offset_to_utf16};
use vmux_ui::components::composer::{PROMPT_INPUT_ID, PromptComposer, focus_prompt_end};
use vmux_ui::components::composer_bar::ComposerBar;
use vmux_ui::components::icon::Icon;
use vmux_ui::components::mcp_menu::{McpMenu, use_mcp_connections};
use vmux_ui::components::prompt_box::{PromptBox, PromptPopup, PromptPopupPlacement};
use vmux_ui::components::prompt_media_options::PromptMediaOptions;
use vmux_ui::hooks::{MenuDirection, send, use_key_claim, use_ui_state, use_ui_state_patches};
use vmux_ui::i18n::translate;
use vmux_ui::ime::use_ime_guard;
use vmux_ui::prompt_recall::{PromptHistoryDirection, prompt_history_direction};
use vmux_ui::scroll::ScrollIntoView;

mod composer;
mod media;
mod panel;
mod readline;
mod row;
mod signals;

pub use panel::CommandBarPanel;
pub use row::ResultRow;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaletteSurface {
    Modal,
    Start,
}

impl PaletteSurface {
    pub const fn is_start(self) -> bool {
        matches!(self, Self::Start)
    }
}

pub fn use_command_bar_ui() -> Signal<CommandBarOpenEvent> {
    let mut state = use_signal(CommandBarOpenEvent::default);
    let mut handled_focus_revision = use_signal(|| 0);
    let _error = use_ui_state_patches::<CommandBarUiState>(move |patch| {
        if let Some(snapshot) =
            <CommandBarUiStatePatch as vmux_api::UiStatePatch<CommandBarOpenEvent>>::payload(patch)
        {
            state.set(snapshot.clone());
            return;
        }
        let Some(effect) = <CommandBarUiStatePatch as vmux_api::UiStatePatch<
            CommandBarFocusEffect,
        >>::payload(patch) else {
            return;
        };
        if effect.revision <= *handled_focus_revision.peek() {
            return;
        }
        handled_focus_revision.set(effect.revision);
        if effect.revision != 0 {
            focus_prompt_input();
        }
    });
    state
}

#[component]
pub fn CommandPalette(props: PaletteProps) -> Element {
    let state = props.state;
    let surface = props.surface;
    let is_start = surface.is_start();
    let on_close = props.on_close;
    let on_activity = props.on_activity;

    let mut signals = use_palette_signals();
    let host_state = use_ui_state::<CommandPaletteState>();
    let mcp = use_mcp_connections();
    let ime = use_ime_guard();
    let mut handled_close = use_signal(|| None);

    use_drop(move || {
        let _ = send(&UiKeyContext { keys: Vec::new() });
    });

    use_effect(move || {
        let opened = state();
        if signals.reopened(opened.open_id) {
            signals.restart(&opened);
        }
    });

    use_effect(move || {
        let opened = state();
        let query = (signals.query)();
        let selected = (signals.selected)() as u32;
        let navigating = (signals.nav_mode)();
        let _ = send(&CommandPaletteDraftRequest {
            open_id: opened.open_id,
            query,
            start: is_start,
            selected,
            navigating,
        });
    });

    use_effect(move || {
        let opened = state();
        if signals.refocus(opened.open_id) {
            if is_start {
                focus_prompt_end(PROMPT_INPUT_ID);
            } else {
                CommandBarField::focus(&opened);
            }
        }
    });

    use_effect(move || {
        signals.watch();
        on_activity.call(());
    });

    let mut handled_attachment = use_signal(|| None);
    use_effect(move || {
        let opened = state();
        let revision = {
            let snapshot = host_state.read();
            if snapshot.open_id != opened.open_id || snapshot.attachment_sequence == 0 {
                return;
            }
            (snapshot.open_id, snapshot.attachment_sequence)
        };
        if Some(revision) == *handled_attachment.peek() {
            return;
        }
        handled_attachment.set(Some(revision));
        focus_prompt_end(PROMPT_INPUT_ID);
    });

    let input_id = if is_start {
        PROMPT_INPUT_ID
    } else {
        COMMAND_BAR_INPUT_ID
    };
    use_effect(move || {
        let opened = state();
        let snapshot = host_state.read();
        if snapshot.open_id != opened.open_id {
            return;
        }
        let projection = &snapshot.projection;
        signals.apply_host_input(
            projection.input_revision,
            &projection.query,
            projection.selected as usize,
            projection.navigating,
            input_id,
        );
    });
    use_effect(move || {
        let opened = state();
        let snapshot = host_state.read();
        if snapshot.open_id != opened.open_id || snapshot.projection.close_revision == 0 {
            return;
        }
        let close = (snapshot.open_id, snapshot.projection.close_revision);
        if Some(close) == *handled_close.peek() {
            return;
        }
        handled_close.set(Some(close));
        on_close.call(());
    });
    let keys = use_key_claim(Unclaimed::Types, move || {
        let mut context = vec!["command-bar".to_string()];
        let snapshot = host_state.read();
        if snapshot.open_id == state().open_id && snapshot.projection.menu.is_some() {
            context.push("command-bar.menu".to_string());
        } else {
            if is_start && snapshot.open_id == state().open_id && snapshot.media_query.is_some() {
                context.push("command-bar.media".to_string());
            }
        }
        context
    });

    let state_val = state();
    let host_snapshot = host_state();
    let palette_data = if host_snapshot.open_id == state_val.open_id {
        host_snapshot
    } else {
        CommandPaletteState::default()
    };
    let projection = std::rc::Rc::new(palette_data.projection.clone());
    let attachments = std::rc::Rc::new(palette_data.attachments.clone());
    let q = (signals.query)();
    let ghost_text = projection.ghost.clone();
    let mcp_open = projection.mcp_open;
    let mcp_entries = std::rc::Rc::new(projection.mcp_entries.clone());
    let mcp_selected = (projection.selected as usize).min(mcp_entries.len().saturating_sub(1));
    let media_menu_open = is_start && palette_data.media_query.is_some();
    let media_entries = palette_data.media_entries.clone();
    let media_sel =
        (palette_data.media_selected as usize).min(media_entries.len().saturating_sub(1));
    let media_loading = palette_data.media_loading;
    let media_options = PromptMedia::options(&media_entries);

    use_effect(move || {
        ScrollIntoView::nearest(&format!("command-bar-item-{}", (signals.selected)()));
    });

    use_effect(move || {
        let snapshot = host_state.read();
        if snapshot.open_id != state().open_id {
            return;
        }
        ScrollIntoView::nearest(&format!("prompt-media-item-{}", snapshot.media_selected));
    });

    let composer = projection.composer.clone();
    let accent = projection.accent_agent.as_deref().map(agent_accent);
    let start_accent = accent.unwrap_or_else(|| agent_accent("vibe"));
    let start_prompt_attachments = PromptMedia::composer_attachments(attachments.as_ref());
    let start_submission_enabled = !q.trim().is_empty() || !attachments.is_empty();
    let chips = ComposerChips::build(&composer, state_val.open_id);
    let opened_menu = ComposerMenuView::kind(projection.menu);

    let start_composer_footer = rsx! {
        ComposerBar {
            opened: opened_menu,
            agent: Some(chips.agent),
            model: chips.model,
            permission: chips.permission,
            project: Some(chips.project),
            branch: chips.branch,
            is_git_repo: composer.is_git_repo,
            workspace_known: !composer.cwd.is_empty(),
            uncommitted: composer.uncommitted,
            ahead: composer.ahead,
        }
    };
    let start_menus = rsx! {
        CommandComposerMenus {
            composer: composer.clone(),
            palette: palette_data.clone(),
            open_id: state_val.open_id,
            opened: projection.menu,
            cursor: projection.menu_cursor as usize,
        }
    };

    let start_keydown = {
        let projection = projection.clone();
        let query = q.clone();
        move |e: KeyboardEvent| {
            if Readline::chord(&e, signals.query, &projection.ghost, PROMPT_INPUT_ID) {
                return;
            }
            if e.key() == Key::Tab {
                keys.on_keydown(&e, |_| false);
                return;
            }

            let ctrl = e.modifiers().contains(Modifiers::CONTROL);
            if !ctrl
                && projection.space_switch
                && query.trim().is_empty()
                && let Some(digit) = TypedDigit::from_event(&e)
                && digit < projection.space_count as usize
            {
                e.prevent_default();
                signals.highlight(digit);
                return;
            }
            let direction = MenuDirection::from_key(&e);
            let go_down = direction == Some(MenuDirection::Next);
            let go_up = direction == Some(MenuDirection::Previous);

            if mcp_open {
                if e.key() == Key::Enter && !e.modifiers().shift() {
                    e.prevent_default();
                    if keys.resolves() {
                        keys.on_keydown(&e, |_| false);
                    } else {
                        let _ = send(&CommandPaletteSubmitRequest {
                            open_id: state().open_id,
                        });
                    }
                    return;
                }
                if go_down || go_up || e.key() == Key::Escape || ctrl && e.code() == Code::KeyC {
                    keys.on_keydown(&e, |_| false);
                    return;
                }
            }

            if projection.menu.is_some() {
                let handled = direction.is_some()
                    || e.key() == Key::Enter && !e.modifiers().shift()
                    || e.key() == Key::Escape
                    || ctrl && e.code() == Code::KeyC;
                if !handled {
                    return;
                }
                e.prevent_default();
                if keys.resolves() {
                    keys.on_keydown(&e, |_| false);
                    return;
                }
                if e.key() == Key::Escape || (ctrl && e.code() == Code::KeyC) {
                    let _ = send(&CommandPaletteMenuDismissRequest {
                        open_id: state().open_id,
                    });
                    return;
                }
                if let Some(direction) = direction {
                    let _ = send(&CommandPaletteMenuMoveRequest {
                        open_id: state().open_id,
                        next: direction == MenuDirection::Next,
                    });
                    return;
                }
                if e.key() == Key::Enter && !e.modifiers().shift() {
                    let _ = send(&CommandPaletteMenuActivateRequest {
                        open_id: state().open_id,
                        index: projection.menu_cursor,
                    });
                    return;
                }
            }

            if media_menu_open {
                let handled = go_down
                    || go_up
                    || e.key() == Key::Enter && !e.modifiers().shift()
                    || e.key() == Key::Escape
                    || ctrl && e.code() == Code::KeyC;
                if keys.resolves() && handled {
                    keys.on_keydown(&e, |_| false);
                    return;
                }
                if go_down || go_up {
                    e.prevent_default();
                    let _ = send(&CommandPaletteMediaMoveRequest {
                        open_id: state().open_id,
                        next: go_down,
                    });
                    return;
                }
                if e.key() == Key::Enter && !e.modifiers().shift() {
                    e.prevent_default();
                    let _ = send(&CommandPaletteMediaActivateRequest {
                        open_id: state().open_id,
                        index: None,
                    });
                    return;
                }
                if e.key() == Key::Escape || ctrl && e.code() == Code::KeyC {
                    e.prevent_default();
                    let _ = send(&CommandPaletteMediaDismissRequest {
                        open_id: state().open_id,
                    });
                    return;
                }
            }

            let wanted = match projection.history_recalling {
                true => PromptHistoryDirection::from_menu(direction),
                false => {
                    let (start, end) = EventSelection::in_field(PROMPT_INPUT_ID);
                    let entering = go_up && projection.selected == 0;
                    entering
                        .then(|| {
                            prompt_history_direction(
                                &e.key().to_string(),
                                ctrl,
                                &query,
                                byte_offset_to_utf16(&query, start),
                                byte_offset_to_utf16(&query, end),
                            )
                        })
                        .flatten()
                }
            };
            if let Some(wanted) = wanted {
                e.prevent_default();
                let _ = send(&CommandPaletteHistoryMoveRequest {
                    open_id: state().open_id,
                    older: matches!(wanted, PromptHistoryDirection::Older),
                });
                return;
            }

            if go_down {
                keys.on_keydown(&e, |_| false);
            } else if go_up {
                keys.on_keydown(&e, |_| false);
            } else if e.key() == Key::Escape || (ctrl && e.code() == Code::KeyC) {
                keys.on_keydown(&e, |_| false);
            } else if e.key() == Key::Enter && !e.modifiers().shift() {
                e.prevent_default();
                if keys.resolves() {
                    keys.on_keydown(&e, |_| false);
                } else {
                    let _ = send(&CommandPaletteSubmitRequest {
                        open_id: state().open_id,
                    });
                }
            }
        }
    };
    let modal_keydown = {
        let projection = projection.clone();
        let query = q.clone();
        move |e: KeyboardEvent| {
            if ime.swallows(&e) {
                return;
            }
            if Readline::chord(&e, signals.query, &projection.ghost, COMMAND_BAR_INPUT_ID) {
                return;
            }
            let ctrl = e.modifiers().contains(Modifiers::CONTROL);
            if !ctrl
                && projection.space_switch
                && query.trim().is_empty()
                && let Some(digit) = TypedDigit::from_event(&e)
                && digit < projection.space_count as usize
            {
                e.prevent_default();
                signals.highlight(digit);
                return;
            }
            if e.key() == Key::Enter {
                e.prevent_default();
                if keys.resolves() {
                    keys.on_keydown(&e, |_| false);
                } else {
                    let _ = send(&CommandPaletteSubmitRequest {
                        open_id: state().open_id,
                    });
                }
                return;
            }
            keys.on_keydown(&e, |_| false);
        }
    };
    let on_send = move |_| {
        let _ = send(&CommandPaletteSubmitRequest {
            open_id: state().open_id,
        });
    };
    let removable_attachments = attachments.clone();

    rsx! {
        div { class: "relative",
            if is_start {
                if let Some(accent) = accent {
                    div { class: "{accent.glow_top} transform-gpu" }
                    div { class: "{accent.glow_bottom} transform-gpu" }
                }
            }
            if is_start {
                {start_menus}
                PromptComposer {
                    shared_transition: true,
                    value: q.clone(),
                    overlay: projection.row_text.clone().unwrap_or_default(),
                    completion: ghost_text.clone(),
                    attachments: start_prompt_attachments,
                    placeholder: translate("command-composer-placeholder"),
                    accent_color: format!("rgb({})", start_accent.rain_rgb),
                    accent_gradient: start_accent.grad.to_string(),
                    footer: Some(start_composer_footer),
                    action_title: translate("command-send"),
                    action_enabled: start_submission_enabled,
                    on_input: move |value| signals.retype(value),
                    on_keydown: start_keydown,
                    on_paste: move |_| {
                        let _ = send(&ChatPasteMedia);
                    },
                    on_attach: move |_| {
                        let _ = send(&ChatPickFiles);
                    },
                    on_remove_attachment: move |index: usize| {
                        let Some(attachment) = removable_attachments.get(index) else {
                            return;
                        };
                        let _ = send(&CommandPaletteRemoveAttachmentRequest {
                            open_id: state().open_id,
                            path: attachment.path.clone(),
                        });
                    },
                    on_action: on_send,
                }
            } else {
                PromptBox {
                    glass: false,
                    class: "p-2",
                    div { class: "flex w-full min-w-0 flex-1 items-center gap-2 overflow-hidden rounded-lg bg-foreground/5 px-3",
                        if !projection.space_name.is_empty() {
                            span {
                                title: "{projection.space_name}",
                                class: "max-w-36 shrink-0 truncate rounded-md bg-glass-hover px-2 py-1 text-ui-xs font-medium text-muted-foreground",
                                "{projection.space_name}"
                            }
                        }
                        PaletteModeChip { mode: projection.mode }
                        if let Some(glyph) = projection.glyph {
                            PaletteGlyphIcon { glyph }
                        }
                        div { class: "relative min-w-0 flex-1 overflow-hidden",
                            if let Some(row_text) = projection.row_text.clone() {
                                div { class: "pointer-events-none absolute inset-0 flex items-center",
                                    span { class: "truncate text-base text-foreground", "{row_text}" }
                                }
                            } else if !ghost_text.is_empty() {
                                div { class: "pointer-events-none absolute inset-0 flex items-center",
                                    span { class: "invisible text-base", "{q}" }
                                    span { class: "text-base text-muted-foreground/40", "{ghost_text}" }
                                }
                            }
                            input {
                                id: "command-bar-input",
                                r#type: "text",
                                "data-ghost": "{ghost_text}",
                                class: if projection.row_text.is_some() {
                                    "w-full min-w-0 cursor-text bg-transparent py-2.5 text-base text-transparent caret-foreground outline-none placeholder:text-muted-foreground"
                                } else {
                                    "w-full min-w-0 cursor-text bg-transparent py-2.5 text-base text-foreground caret-foreground outline-none placeholder:text-muted-foreground"
                                },
                                placeholder: if projection.row_text.is_some() { String::new() } else { projection.placeholder.clone() },
                                value: "{q}",
                                autofocus: true,
                                oninput: move |event| signals.retype(event.value()),
                                oncompositionstart: move |_| ime.start(),
                                oncompositionend: move |_| ime.commit(),
                                onkeydown: modal_keydown,
                            }
                        }
                        BookmarkButton {}
                    }
                }
            }
            if projection.menu.is_none() && mcp_open {
                McpMenu {
                    connections: mcp,
                    entries: mcp_entries.as_ref().clone(),
                    selected: mcp_selected,
                    placement: if is_start { PromptPopupPlacement::Downward } else { PromptPopupPlacement::Inline },
                    on_select: move |index| {
                        let _ = send(&CommandPaletteActivateRequest {
                            open_id: state().open_id,
                            index: index as u32,
                        });
                    },
                    on_hover: move |index| signals.selected.set(index),
                    on_dismiss: move |()| signals.retype(String::new()),
                }
            }
            if projection.menu.is_none() && !mcp_open && media_menu_open {
                PromptPopup {
                    placement: PromptPopupPlacement::Downward,
                    id: "command-bar-results",
                    PromptMediaOptions {
                        items: media_options,
                        selected: media_sel,
                        loading: media_loading,
                        loading_label: translate("agent-loading-media"),
                        empty_label: translate("agent-no-matching-media"),
                        on_hover: move |index| {
                            let _ = send(&CommandPaletteMediaHighlightRequest {
                                open_id: state().open_id,
                                index: index as u32,
                            });
                        },
                        on_select: move |index| {
                            let _ = send(&CommandPaletteMediaActivateRequest {
                                open_id: state().open_id,
                                index: Some(index as u32),
                            });
                        },
                    }
                }
            }
            if projection.menu.is_none() && !mcp_open && !media_menu_open && !projection.rows.is_empty() {
                PromptPopup {
                    placement: if is_start { PromptPopupPlacement::Downward } else { PromptPopupPlacement::Inline },
                    id: "command-bar-results",
                    class: if is_start { "" } else { "max-h-80 overflow-x-hidden overflow-y-auto border-t border-border" },
                for (i, item) in projection.rows.iter().enumerate() {
                    ResultRow {
                        key: "{i}",
                        index: i,
                        item: item.clone(),
                        selected: i == projection.selected as usize,
                        on_activate: move |_| {
                            let _ = send(&CommandPaletteActivateRequest {
                                open_id: state().open_id,
                                index: i as u32,
                            });
                        },
                        space_switch: projection.space_switch,
                        on_hover: move |_| {
                            if is_start {
                                signals.selected.set(i);
                            }
                        },
                    }
                }
                }
            }
        }
    }
}

#[component]
fn PaletteModeChip(mode: PaletteMode) -> Element {
    let id = match mode {
        PaletteMode::Ex => "palette-mode-ex",
        PaletteMode::Command => "palette-mode-command",
        PaletteMode::Path => "palette-mode-path",
        PaletteMode::Slash => "palette-mode-slash",
        PaletteMode::Picking(picker) => match picker {
            CommandBarPicker::Space => "",
            CommandBarPicker::GotoLine => "editor-status-goto-title",
            CommandBarPicker::Indent => "editor-status-indent-title",
            CommandBarPicker::LineEnding => "editor-status-eol-title",
            CommandBarPicker::Encoding => "editor-status-encoding-title",
            CommandBarPicker::EncodingReopen => "editor-status-encoding-reopen",
            CommandBarPicker::EncodingSave => "editor-status-encoding-save",
        },
        PaletteMode::Search | PaletteMode::Url => "",
    };
    let label = if id.is_empty() {
        String::new()
    } else {
        translate(id)
    };
    rsx! {
        if !label.is_empty() {
            span {
                class: "shrink-0 rounded-md bg-accent/15 px-2 py-1 text-ui-xs font-medium text-accent-foreground",
                "{label}"
            }
        }
    }
}

#[component]
fn PaletteGlyphIcon(glyph: PaletteGlyph) -> Element {
    let class = "h-4 w-4 shrink-0 text-muted-foreground";
    match glyph {
        PaletteGlyph::Command => rsx! {
            span { class: "select-none font-mono text-base text-muted-foreground", ">_" }
        },
        PaletteGlyph::Path => rsx! {
            Icon { class,
                path { d: "M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z" }
                path { d: "M14 2v4a2 2 0 0 0 2 2h4" }
            }
        },
        PaletteGlyph::Url => rsx! {
            Icon { class,
                path { d: "M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20Z" }
                path { d: "M2 12h20" }
                path { d: "M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10Z" }
            }
        },
        PaletteGlyph::Search => rsx! {
            Icon { class,
                circle { cx: "11", cy: "11", r: "8" }
                path { d: "m21 21-4.3-4.3" }
            }
        },
    }
}

#[component]
fn BookmarkButton() -> Element {
    rsx! {
        button {
            r#type: "button",
            aria_label: translate("layout-bookmark-page"),
            title: format!("{} (⌘D)", translate("layout-bookmark-page")),
            class: "flex h-7 w-7 shrink-0 items-center justify-center rounded-md text-muted-foreground hover:bg-foreground/10 hover:text-foreground",
            onmousedown: move |event| {
                event.prevent_default();
                event.stop_propagation();
            },
            onclick: move |event| {
                event.prevent_default();
                event.stop_propagation();
                let _ = send(&vmux_api::bookmark::BookmarkToggleRequest);
            },
            Icon { class: "h-4 w-4",
                path { d: "M19 21l-7-5-7 5V5a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2z" }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct PaletteProps {
    pub state: ReadSignal<CommandBarOpenEvent>,
    pub surface: PaletteSurface,
    pub on_close: EventHandler<()>,
    pub on_activity: EventHandler<()>,
}

pub fn focus_prompt_input() {
    focus_prompt_end(PROMPT_INPUT_ID);
}
