use crate::event::{
    CommandBarFocusInput, CommandBarKey, CommandBarOpenEvent, CommandBarQuery, CommandBarUiState,
    CommandBarUiStatePatch, CommandPaletteBranchesRequest, CommandPaletteDraftRequest,
    CommandPalettePromptHistoryRequest, CommandPaletteSelectionRequest, CommandPaletteState,
};
use crate::prompt_media::{ChatPasteMedia, ChatPickFiles, inline_media_query};
use crate::ui::composer::{ComposerChips, ComposerMenuSet, use_prompt_recall};
use crate::ui::media::{PromptMedia, use_prompt_media};
use crate::ui::signals::{
    COMMAND_BAR_INPUT_ID, CommandBarField, Readline, TypedDigit, use_palette_signals,
};
use dioxus::prelude::*;
use vmux_core::input::{PageKeyContext, Unclaimed};
use vmux_ui::agent_accent::agent_accent;
use vmux_ui::caret::{EventSelection, byte_offset_to_utf16};
use vmux_ui::components::composer::{PROMPT_INPUT_ID, PromptComposer, focus_prompt_end};
use vmux_ui::components::composer_bar::{ComposerBar, ComposerMenus, use_composer_menu};
use vmux_ui::components::icon::Icon;
use vmux_ui::components::mcp_menu::{McpMenu, use_mcp_connections};
use vmux_ui::components::prompt_box::{PromptBox, PromptPopup, PromptPopupPlacement};
use vmux_ui::components::prompt_media_options::PromptMediaOptions;
use vmux_ui::hooks::{
    MenuDirection, move_selection, send, use_key_claim, use_ui_state, use_ui_state_patch,
};
use vmux_ui::i18n::translate;
use vmux_ui::ime::use_ime_guard;
use vmux_ui::launcher::palette::{
    PaletteGlyph, PaletteRows, PaletteState, PaletteSurface, Submission,
};
use vmux_ui::launcher::row::ResultRow;
use vmux_ui::launcher::style::{
    command_bar_input_class, command_bar_input_row_class, command_bar_input_wrap_class,
    command_bar_row_overlay_class, result_list_class,
};
use vmux_ui::prompt_recall::{PromptHistoryDirection, prompt_history_direction};
use vmux_ui::scroll::ScrollIntoView;

mod composer;
mod media;
mod signals;

pub fn use_command_bar_ui() -> Signal<CommandBarOpenEvent> {
    let root = vmux_ui::hooks::use_ui_state_root::<CommandBarUiState>();
    let mut state = use_signal(CommandBarOpenEvent::default);
    let mut handled_sequence = use_signal(|| 0);
    use_effect(move || {
        let event = root.state.read();
        if event.sequence == 0 || event.sequence == *handled_sequence.peek() {
            return;
        }
        handled_sequence.set(event.sequence);
        for patch in &event.patches {
            if let Some(snapshot) = <CommandBarUiStatePatch as vmux_api::UiStatePatch<
                CommandBarOpenEvent,
            >>::payload(patch)
            {
                state.set(snapshot.clone());
                continue;
            }
            if <CommandBarUiStatePatch as vmux_api::UiStatePatch<CommandBarFocusInput>>::payload(
                patch,
            )
            .is_some()
            {
                focus_prompt_input();
            }
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
    let on_dismiss = props.on_dismiss;
    let on_activity = props.on_activity;
    let on_start_inline_transition = props.on_start_inline_transition;

    let mut signals = use_palette_signals();
    let host_state = use_ui_state::<CommandPaletteState>();
    let open_id = state().open_id;
    let mut media = use_prompt_media(host_state, open_id);
    let menu = use_composer_menu();
    let mcp = use_mcp_connections();
    let ime = use_ime_guard();

    use_drop(move || {
        let _ = send(&PageKeyContext { keys: Vec::new() });
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
        let target_url = (signals.target_url)();
        let selected = (signals.selected)() as u32;
        let navigating = (signals.nav_mode)();
        let _ = send(&CommandPaletteDraftRequest {
            open_id: opened.open_id,
            query,
            start: is_start,
            target_url,
            selected,
            navigating,
        });
    });

    use_effect(move || {
        let opened = state();
        let selected = (signals.selected)() as u32;
        let navigating = (signals.nav_mode)();
        let _ = send(&CommandPaletteSelectionRequest {
            open_id: opened.open_id,
            selected,
            navigating,
        });
    });

    use_effect(move || {
        let query = (signals.query)();
        media.sync_query(&query);
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

    let mut recall = use_prompt_recall();
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

    use_effect(move || {
        let query = (signals.query)();
        if CommandBarQuery(&query).mcp_filter().is_some() {
            mcp.request();
        }
    });

    let rows = use_memo(move || {
        let opened = state();
        let snapshot = host_state.read();
        if snapshot.open_id != opened.open_id {
            return PaletteRows::default();
        }
        PaletteRows::from_projection(&snapshot.projection)
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
    let keys = use_key_claim(Unclaimed::Types, || vec!["command-bar".to_string()]);
    let key_updates = use_ui_state_patch::<CommandBarUiState, CommandBarKey>();
    use_effect(move || {
        for key in key_updates.take() {
            let query = signals.query.peek().clone();
            let command_bar_query = CommandBarQuery(&query);
            let Some(filter) = command_bar_query.mcp_filter() else {
                continue;
            };
            let entries = mcp.filtered(filter);
            match key {
                CommandBarKey::Next => {
                    let current = *signals.selected.peek();
                    signals.highlight(move_selection(current, entries.len(), MenuDirection::Next));
                }
                CommandBarKey::Previous => {
                    let current = *signals.selected.peek();
                    signals.highlight(move_selection(
                        current,
                        entries.len(),
                        MenuDirection::Previous,
                    ));
                }
                CommandBarKey::Complete => {}
                CommandBarKey::Dismiss => signals.retype(String::new()),
            }
        }
    });

    let state_val = state();
    let host_snapshot = host_state();
    let palette_data = if host_snapshot.open_id == state_val.open_id {
        host_snapshot
    } else {
        CommandPaletteState::default()
    };
    let palette = std::rc::Rc::new(PaletteState::from_rows(
        &rows(),
        &state_val,
        &signals.draft(),
        surface,
    ));
    let query = signals.query;
    let attachments = std::rc::Rc::new(palette_data.attachments.clone());
    let q = palette.query.clone();
    let ghost_text = palette.ghost.clone();
    let mcp_query = CommandBarQuery(&q).mcp_filter().map(str::to_string);
    let mcp_open = mcp_query.is_some();
    let mcp_entries = std::rc::Rc::new(
        mcp_query
            .as_deref()
            .map(|query| mcp.filtered(query))
            .unwrap_or_default(),
    );
    let mcp_selected = (*signals.selected.peek()).min(mcp_entries.len().saturating_sub(1));
    let media_menu_open = is_start && inline_media_query(&q).is_some();
    let media_entries = media.entries(&q);
    let media_sel = media.highlighted(media_entries.len());
    let media_options = PromptMedia::options(&media_entries);

    use_effect(move || {
        ScrollIntoView::nearest(&format!("command-bar-item-{}", (signals.selected)()));
    });

    use_effect(move || {
        let _ = host_state.read().media_entries.len();
        ScrollIntoView::nearest(&format!("prompt-media-item-{}", (media.selected)()));
    });

    let apply_attachments = attachments.clone();
    let apply = std::rc::Rc::new(move |submission: Submission| {
        if let Some(typed) = submission.retype {
            let mut signals = signals;
            signals.retype(typed);
            focus_prompt_end(PROMPT_INPUT_ID);
            return;
        }
        if submission.close {
            on_close.call(());
        }
        let _ = submission.send();
        let (Some(target_url), Some(handler)) =
            (submission.inline_target, on_start_inline_transition)
        else {
            return;
        };
        handler.call(StartInlineTransition {
            target_url,
            prompt: query.peek().trim().to_string(),
            attachments: apply_attachments.as_ref().clone(),
        });
    });

    let composer = palette.composer.clone();
    {
        let agent = vmux_ui::launcher::palette::AgentSegment::in_url(&composer.agent_url)
            .unwrap_or_default();
        let cwd = composer.cwd.clone();
        let open_id = state_val.open_id;
        use_effect(move || {
            let _ = state();
            let _ = (signals.target_url)();
            let _ = (signals.selected)();
            let _ = (signals.nav_mode)();
            let _ = send(&CommandPalettePromptHistoryRequest {
                open_id,
                agent: agent.clone(),
                cwd: cwd.clone(),
            });
        });
    }
    {
        let project = composer.project.clone();
        let open_id = state_val.open_id;
        use_effect(move || {
            let _ = state();
            let _ = (signals.target_url)();
            let _ = (signals.selected)();
            let _ = (signals.nav_mode)();
            let _ = send(&CommandPaletteBranchesRequest {
                open_id,
                project: project.clone(),
            });
        });
    }
    let accent = palette.accent_agent.as_deref().map(agent_accent);
    let start_accent = accent.unwrap_or_else(|| agent_accent("vibe"));
    let start_prompt_attachments = PromptMedia::composer_attachments(attachments.as_ref());
    let start_submission_enabled = !q.trim().is_empty() || !attachments.is_empty();
    let prompt_history = std::rc::Rc::new(palette_data.prompt_history.clone());
    let chips = ComposerChips::build(&composer, menu);
    let menus = ComposerMenuSet::build(&composer, signals, &palette_data);
    let start_composer_footer = rsx! {
        ComposerBar {
            menu,
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
        ComposerMenus {
            menu,
            placement: PromptPopupPlacement::Downward,
            agent: Some(menus.agent.clone()),
            model: Some(menus.model.clone()),
            permission: Some(menus.permission.clone()),
            project: Some(menus.project.clone()),
            branch: Some(menus.branch.clone()),
        }
    };

    let start_keydown = {
        let apply = apply.clone();
        let attachments = attachments.clone();
        let palette = palette.clone();
        let menus = menus.clone();
        let entries = mcp_entries.clone();
        let prompt_history = prompt_history.clone();
        move |e: KeyboardEvent| {
            if Readline::chord(&e, signals.query, &palette.ghost, PROMPT_INPUT_ID) {
                return;
            }
            if e.key() == Key::Tab {
                e.prevent_default();
                if !palette.ghost.is_empty() {
                    signals
                        .query
                        .set(format!("{}{}", palette.query, palette.ghost));
                    signals.selected.set(0);
                    focus_prompt_end(PROMPT_INPUT_ID);
                }
                return;
            }

            let ctrl = e.modifiers().contains(Modifiers::CONTROL);
            if !ctrl
                && palette.space_switch
                && palette.query.trim().is_empty()
                && let Some(digit) = TypedDigit::from_event(&e)
                && let Some(index) = palette.space_digit(digit)
            {
                e.prevent_default();
                signals.highlight(index);
                return;
            }
            let direction = MenuDirection::from_key(&e);
            let go_down = direction == Some(MenuDirection::Next);
            let go_up = direction == Some(MenuDirection::Previous);

            if mcp_open {
                if e.key() == Key::Escape || (ctrl && e.code() == Code::KeyC) {
                    e.prevent_default();
                    signals.retype(String::new());
                    return;
                }
                if let Some(direction) = direction {
                    e.prevent_default();
                    let current = *signals.selected.peek();
                    signals.highlight(move_selection(current, entries.len(), direction));
                    return;
                }
                if e.key() == Key::Enter && !e.modifiers().shift() {
                    e.prevent_default();
                    if let Some(server) = entries.get(mcp_selected) {
                        mcp.activate(server);
                    }
                    return;
                }
            }

            if let Some(kind) = menu.opened() {
                if e.key() == Key::Escape || (ctrl && e.code() == Code::KeyC) {
                    e.prevent_default();
                    menu.close();
                    return;
                }
                if let Some(direction) = direction {
                    e.prevent_default();
                    menu.step(direction, menus.rows(kind));
                    return;
                }
                if e.key() == Key::Enter && !e.modifiers().shift() {
                    e.prevent_default();
                    if menus.choose(kind, menu.cursor()) {
                        menu.close();
                    }
                    return;
                }
            }

            if media_menu_open && media.handle_key(&e, go_down, go_up, signals.query) {
                return;
            }

            let wanted = match recall.recalling(&palette.query) {
                true => PromptHistoryDirection::from_menu(direction),
                false => {
                    let (start, end) = EventSelection::in_field(PROMPT_INPUT_ID);
                    let entering = go_up && palette.selected == 0;
                    entering
                        .then(|| {
                            prompt_history_direction(
                                &e.key().to_string(),
                                ctrl,
                                &palette.query,
                                byte_offset_to_utf16(&palette.query, start),
                                byte_offset_to_utf16(&palette.query, end),
                            )
                        })
                        .flatten()
                }
            };
            if let Some(wanted) = wanted
                && let Some(value) = recall.walk(prompt_history.as_ref(), wanted, &palette.query)
            {
                e.prevent_default();
                signals.retype(value);
                focus_prompt_end(PROMPT_INPUT_ID);
                return;
            }

            if go_down {
                keys.on_keydown(&e, |_| false);
            } else if go_up {
                keys.on_keydown(&e, |_| false);
            } else if e.key() == Key::Escape || (ctrl && e.code() == Code::KeyC) {
                on_dismiss.call(());
            } else if e.key() == Key::Enter && !e.modifiers().shift() {
                e.prevent_default();
                apply(palette.submit_start(attachments.as_ref()));
            }
        }
    };
    let modal_keydown = {
        let apply = apply.clone();
        let attachments = attachments.clone();
        let palette = palette.clone();
        let entries = mcp_entries.clone();
        move |e: KeyboardEvent| {
            if ime.swallows(&e) {
                return;
            }
            if Readline::chord(&e, signals.query, &palette.ghost, COMMAND_BAR_INPUT_ID) {
                return;
            }
            let ctrl = e.modifiers().contains(Modifiers::CONTROL);
            if !ctrl
                && palette.space_switch
                && palette.query.trim().is_empty()
                && let Some(digit) = TypedDigit::from_event(&e)
                && let Some(index) = palette.space_digit(digit)
            {
                e.prevent_default();
                signals.highlight(index);
                return;
            }
            if e.key() == Key::Enter {
                if mcp_open {
                    e.prevent_default();
                    if let Some(server) = entries.get(mcp_selected) {
                        mcp.activate(server);
                    }
                    return;
                }
                apply(palette.submit_modal(attachments.as_ref()));
                return;
            }
            keys.on_keydown(&e, |_| false);
        }
    };
    let on_send = {
        let apply = apply.clone();
        let attachments = attachments.clone();
        let palette = palette.clone();
        let entries = mcp_entries.clone();
        move |_| {
            if mcp_open {
                if let Some(server) = entries.get(mcp_selected) {
                    mcp.activate(server);
                }
                return;
            }
            apply(palette.submit_current(attachments.as_ref()));
        }
    };

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
                    overlay: palette.row_text.clone().unwrap_or_default(),
                    completion: ghost_text.clone(),
                    attachments: start_prompt_attachments,
                    placeholder: translate("command-composer-placeholder"),
                    accent_color: format!("rgb({})", start_accent.rain_rgb),
                    accent_gradient: start_accent.grad.to_string(),
                    footer: Some(start_composer_footer),
                    action_title: translate("command-send"),
                    action_enabled: start_submission_enabled,
                    on_input: move |value| {
                        menu.close();
                        signals.retype(value);
                    },
                    on_keydown: start_keydown,
                    on_paste: move |_| {
                        let _ = send(&ChatPasteMedia);
                    },
                    on_attach: move |_| {
                        let _ = send(&ChatPickFiles);
                    },
                    on_remove_attachment: move |index| media.remove_attachment(index),
                    on_action: on_send,
                }
            } else {
                PromptBox {
                    glass: false,
                    class: "p-2",
                    div { class: command_bar_input_row_class(),
                        if !palette.space_name.is_empty() {
                            span {
                                title: "{palette.space_name}",
                                class: "max-w-36 shrink-0 truncate rounded-md bg-glass-hover px-2 py-1 text-ui-xs font-medium text-muted-foreground",
                                "{palette.space_name}"
                            }
                        }
                        PaletteModeChip { mode: palette.mode }
                        if let Some(glyph) = palette.glyph {
                            PaletteGlyphIcon { glyph }
                        }
                        div { class: command_bar_input_wrap_class(),
                            if let Some(row_text) = palette.row_text.clone() {
                                div { class: command_bar_row_overlay_class(),
                                    span { class: "truncate text-base text-foreground", "{row_text}" }
                                }
                            } else if !ghost_text.is_empty() {
                                div { class: command_bar_row_overlay_class(),
                                    span { class: "invisible text-base", "{q}" }
                                    span { class: "text-base text-muted-foreground/40", "{ghost_text}" }
                                }
                            }
                            input {
                                id: "command-bar-input",
                                r#type: "text",
                                "data-ghost": "{ghost_text}",
                                class: command_bar_input_class(palette.row_text.is_some()),
                                placeholder: if palette.row_text.is_some() { String::new() } else { palette.placeholder.clone() },
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
            if menu.opened().is_none() && mcp_open {
                McpMenu {
                    connections: mcp,
                    entries: mcp_entries.as_ref().clone(),
                    selected: mcp_selected,
                    placement: if is_start { PromptPopupPlacement::Downward } else { PromptPopupPlacement::Inline },
                    on_select: {
                        let entries = mcp_entries.clone();
                        move |index| {
                            if let Some(server) = entries.get(index) {
                                mcp.activate(server);
                            }
                        }
                    },
                    on_hover: move |index| signals.selected.set(index),
                    on_dismiss: move |()| signals.retype(String::new()),
                }
            }
            if menu.opened().is_none() && !mcp_open && media_menu_open {
                PromptPopup {
                    placement: PromptPopupPlacement::Downward,
                    id: "command-bar-results",
                    PromptMediaOptions {
                        items: media_options,
                        selected: media_sel,
                        loading: media.loading(&q),
                        loading_label: translate("agent-loading-media"),
                        empty_label: translate("agent-no-matching-media"),
                        on_hover: move |index| media.selected.set(index),
                        on_select: move |index| media.pick_at(index, signals.query),
                    }
                }
            }
            if menu.opened().is_none() && !mcp_open && !media_menu_open && !palette.rows.is_empty() {
                PromptPopup {
                    placement: if is_start { PromptPopupPlacement::Downward } else { PromptPopupPlacement::Inline },
                    id: "command-bar-results",
                    class: if is_start { "" } else { result_list_class() },
                for (i, item) in palette.rows.iter().enumerate() {
                    ResultRow {
                        key: "{i}",
                        index: i,
                        item: item.clone(),
                        selected: i == palette.selected,
                        on_activate: {
                            let apply = apply.clone();
                            let attachments = attachments.clone();
                            let palette = palette.clone();
                            let item = item.clone();
                            move |_| apply(palette.activate(&item, attachments.as_ref()))
                        },
                        space_switch: palette.space_switch,
                        start_prompt_mode: palette.start_prompt_mode,
                        query: q.clone(),
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
fn PaletteModeChip(mode: vmux_api::command_bar::PaletteMode) -> Element {
    let id = mode.label();
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

#[derive(Clone, PartialEq)]
pub struct StartInlineTransition {
    pub target_url: String,
    pub prompt: String,
    pub attachments: Vec<crate::prompt_media::ChatAttachment>,
}

#[derive(Props, Clone, PartialEq)]
pub struct PaletteProps {
    pub state: ReadSignal<CommandBarOpenEvent>,
    pub surface: PaletteSurface,
    pub on_close: EventHandler<()>,
    pub on_dismiss: EventHandler<()>,
    pub on_activity: EventHandler<()>,
    #[props(default)]
    pub on_start_inline_transition: Option<EventHandler<StartInlineTransition>>,
}

pub fn focus_prompt_input() {
    focus_prompt_end(PROMPT_INPUT_ID);
}
