use crate::event::{
    COMMAND_BAR_KEY_EVENT, CommandBarKey, CommandBarOpenEvent, START_PROJECT_BRANCHES_EVENT,
    StartProjectBranches,
};
use crate::page::composer::{
    ComposerChips, ComposerMenuSet, use_project_picking, use_prompt_recall,
};
use crate::page::media::use_prompt_media;
use crate::page::search::{use_host_search, use_palette_feeds};
use crate::page::signals::{
    COMMAND_BAR_INPUT_ID, CommandBarField, PaletteKeys, Readline, TypedDigit, use_palette_signals,
};
use crate::prompt_media::{
    CHAT_ATTACHMENT_PREVIEWS_EVENT, CHAT_ATTACHMENTS_EVENT, CHAT_MEDIA_ENTRIES_EVENT,
    ChatAttachments, ChatMediaEntries, ChatPasteMedia, ChatPickFiles, inline_media_query,
    merge_chat_attachments,
};
use dioxus::prelude::*;
use vmux_core::input::{PageKeyContext, Unclaimed};
use vmux_ui::agent_accent::agent_accent;
use vmux_ui::caret::{EventSelection, byte_offset_to_utf16};
use vmux_ui::components::composer::{PROMPT_INPUT_ID, PromptComposer, focus_prompt_end};
use vmux_ui::components::composer_bar::{ComposerBar, ComposerMenus, use_composer_menu};
use vmux_ui::components::icon::Icon;
use vmux_ui::components::mcp_menu::{McpMenu, McpQuery, use_mcp_connections};
use vmux_ui::components::prompt_box::{PromptBox, PromptPopup, PromptPopupPlacement};
use vmux_ui::components::prompt_media_options::PromptMediaOptions;
use vmux_ui::hooks::{MenuDirection, move_selection, send, use_key_claim, use_listener};
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
use vmux_wire::chat::{
    PROMPT_HISTORY_EVENT, PromptHistory, RESUMABLE_SESSIONS_EVENT, ResumableSessions,
    ResumeListRequest,
};
use vmux_wire::command_bar::CommandBarQuery;

mod composer;
mod media;
mod search;
mod signals;

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
    let feeds = use_palette_feeds();
    let mut media = use_prompt_media();
    let search = use_host_search();
    let menu = use_composer_menu();
    let mcp = use_mcp_connections();
    let ime = use_ime_guard();

    let keys = use_key_claim(Unclaimed::Types, move || match surface {
        PaletteSurface::Modal => vec!["command-bar".to_string()],
        PaletteSurface::Start => Vec::new(),
    });
    use_drop(move || {
        let _ = send(&PageKeyContext { keys: Vec::new() });
    });

    use_effect(move || {
        let opened = state();
        if signals.reopened(opened.open_id) {
            signals.restart(&opened);
            feeds.clear();
            if is_start {
                media.reset();
            }
        }
    });

    feeds.listen(signals, &search, surface);
    media.listen(signals, &search, surface);
    use_drop({
        let search = search.clone();
        move || search.cancel_all()
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
        feeds.watch();
        on_activity.call(());
    });

    let mut recall = use_prompt_recall();
    let _prompt_history = use_listener::<PromptHistory, _>(PROMPT_HISTORY_EVENT, move |incoming| {
        recall.remember(incoming.prompts);
    });

    let mut picking = use_project_picking();
    let _project_branches =
        use_listener::<StartProjectBranches, _>(START_PROJECT_BRANCHES_EVENT, move |incoming| {
            picking.remember(incoming.project, incoming.branches);
        });

    use_effect(move || {
        picking.read_ahead(&vmux_ui::launcher::palette::ActiveProject::of(
            &state().prompt_context,
        ));
    });

    let _sessions =
        use_listener::<ResumableSessions, _>(RESUMABLE_SESSIONS_EVENT, move |incoming| {
            let mut sessions = feeds.sessions;
            let mut total = feeds.sessions_total;
            let mut loading = feeds.sessions_loading;
            total.set(incoming.total);
            loading.set(false);
            if incoming.offset == 0 {
                sessions.set(incoming.sessions.clone());
                return;
            }
            let mut held = sessions.peek().clone();
            held.extend(incoming.sessions.iter().cloned());
            sessions.set(held);
        });
    use_effect(move || {
        let query = (signals.query)();
        let wants = CommandBarQuery(&query)
            .slash_token()
            .is_some_and(|(name, _)| "resume".starts_with(&name.to_lowercase()));
        let mut asked = feeds.sessions_asked;
        if !wants {
            asked.set(false);
            return;
        }
        if *asked.peek() {
            return;
        }
        asked.set(true);
        let mut sessions = feeds.sessions;
        let mut loading = feeds.sessions_loading;
        sessions.set(Vec::new());
        loading.set(true);
        let _ = send(&ResumeListRequest { offset: 0 });
    });
    use_effect(move || {
        let query = (signals.query)();
        if McpQuery::read(&query).is_some() {
            mcp.request();
        }
    });
    use_effect(move || {
        let selected = (signals.selected)() as u32;
        let loaded = feeds.sessions.read().len() as u32;
        let total = (feeds.sessions_total)();
        if loaded == 0 || loaded >= total || *feeds.sessions_loading.peek() {
            return;
        }
        if selected + 10 < loaded {
            return;
        }
        let mut loading = feeds.sessions_loading;
        loading.set(true);
        let _ = send(&ResumeListRequest { offset: loaded });
    });

    let rows = use_memo(move || PaletteRows::of(&state(), &feeds.draft(signals), surface));
    let mut palette_keys = PaletteKeys {
        rows,
        signals,
        on_dismiss,
    };
    let _key_listener = use_listener::<CommandBarKey, _>(COMMAND_BAR_KEY_EVENT, move |key| {
        let query = signals.query.peek().clone();
        if let Some(filter) = McpQuery::read(&query) {
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
            return;
        }
        palette_keys.apply(key);
    });

    let state_val = state();
    let palette = std::rc::Rc::new(PaletteState::of(
        &rows(),
        &state_val,
        &signals.draft(),
        surface,
    ));
    let query = signals.query;
    let mut attachments = media.attachments;
    let q = palette.query.clone();
    let ghost_text = palette.ghost.clone();
    let mcp_query = McpQuery::read(&q).map(str::to_string);
    let mcp_open = mcp_query.is_some();
    let mcp_entries = std::rc::Rc::new(
        mcp_query
            .as_deref()
            .map(|query| mcp.filtered(query))
            .unwrap_or_default(),
    );
    let mcp_selected = (*signals.selected.peek()).min(mcp_entries.len().saturating_sub(1));
    let media_menu_open = is_start && inline_media_query(&q).is_some();
    let media_sel = media.highlighted();

    use_effect(move || {
        ScrollIntoView::nearest(&format!("command-bar-item-{}", (signals.selected)()));
    });

    use_effect(move || {
        let _ = media.entries.read().len();
        ScrollIntoView::nearest(&format!("prompt-media-item-{}", (media.selected)()));
    });

    let apply = move |submission: Submission| {
        if let Some(typed) = submission.retype {
            let mut signals = signals;
            signals.retype(typed);
            focus_prompt_end(PROMPT_INPUT_ID);
            return;
        }
        if submission.close {
            on_close.call(());
        }
        if let Some(action) = submission.action.as_ref() {
            let _ = send(action);
        }
        let (Some(target_url), Some(handler)) =
            (submission.inline_target, on_start_inline_transition)
        else {
            return;
        };
        handler.call(StartInlineTransition {
            target_url,
            prompt: query.peek().trim().to_string(),
            attachments: attachments.peek().clone(),
        });
    };

    let _attachments_listener =
        use_listener::<ChatAttachments, _>(CHAT_ATTACHMENTS_EVENT, move |selected| {
            if !is_start {
                return;
            }
            let current = attachments.peek().clone();
            attachments.set(merge_chat_attachments(&current, &selected.attachments));
            focus_prompt_end(PROMPT_INPUT_ID);
        });

    let _attachment_previews_listener =
        use_listener::<ChatAttachments, _>(CHAT_ATTACHMENT_PREVIEWS_EVENT, move |loaded| {
            if !is_start {
                return;
            }
            media.remember_previews(&loaded.attachments);
        });

    let _media_entries_listener =
        use_listener::<ChatMediaEntries, _>(CHAT_MEDIA_ENTRIES_EVENT, move |response| {
            if !is_start {
                return;
            }
            media.receive(response);
        });

    let composer = palette.composer.clone();
    {
        let agent = vmux_ui::launcher::palette::AgentSegment::in_url(&composer.agent_url)
            .unwrap_or_default();
        let cwd = composer.cwd.clone();
        use_effect(move || recall.read_ahead(&agent, &cwd));
    }
    let accent = palette.accent_agent.as_deref().map(agent_accent);
    let start_accent = accent.unwrap_or_else(|| agent_accent("vibe"));
    let start_prompt_attachments = media.composer_attachments();
    let start_action_enabled = !q.trim().is_empty() || !attachments.read().is_empty();
    let chips = ComposerChips::of(&composer, menu, picking);
    let menus = ComposerMenuSet::of(&composer, signals, picking);
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
        let palette = palette.clone();
        let menus = menus.clone();
        let entries = mcp_entries.clone();
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
                && let Some(digit) = TypedDigit::of(&e)
                && let Some(index) = palette.space_digit(digit)
            {
                e.prevent_default();
                signals.highlight(index);
                return;
            }
            let direction = MenuDirection::of(&e);
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
                true => PromptHistoryDirection::of(direction),
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
                && let Some(value) = recall.walk(wanted, &palette.query)
            {
                e.prevent_default();
                signals.retype(value);
                focus_prompt_end(PROMPT_INPUT_ID);
                return;
            }

            if go_down {
                e.prevent_default();
                signals.highlight(palette.step(MenuDirection::Next));
            } else if go_up {
                e.prevent_default();
                signals.highlight(palette.step(MenuDirection::Previous));
            } else if e.key() == Key::Escape || (ctrl && e.code() == Code::KeyC) {
                on_dismiss.call(());
            } else if e.key() == Key::Enter && !e.modifiers().shift() {
                e.prevent_default();
                apply(palette.submit_start(&attachments.peek()));
            }
        }
    };
    let modal_keydown = {
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
                && let Some(digit) = TypedDigit::of(&e)
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
                apply(palette.submit_modal(&attachments.peek()));
                return;
            }
            keys.on_keydown(&e, |_| false);
        }
    };
    let on_send = {
        let palette = palette.clone();
        let entries = mcp_entries.clone();
        move |_| {
            if mcp_open {
                if let Some(server) = entries.get(mcp_selected) {
                    mcp.activate(server);
                }
                return;
            }
            apply(palette.submit_action(&attachments.peek()));
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
                    action_enabled: start_action_enabled,
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
                        PaletteModeChip { label: palette.mode.label() }
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
                        items: media.options(),
                        selected: media_sel,
                        loading: (media.loading)(),
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
                            let palette = palette.clone();
                            let item = item.clone();
                            move |_| apply(palette.activate(&item, &attachments.peek()))
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
fn PaletteModeChip(label: String) -> Element {
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
                let _ = send(&crate::event::BookmarksCommandEvent {
                    command: "toggle_active".into(),
                    uuid: None,
                    name: None,
                    url: None,
                    metadata: None,
                    folder: None,
                    target_uuid: None,
                });
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
