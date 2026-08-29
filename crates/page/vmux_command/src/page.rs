use crate::event::{
    COMMAND_BAR_KEY_EVENT, CommandBarKey, CommandBarOpenEvent, START_PROJECT_BRANCHES_EVENT,
    StartProjectBranches,
};
use crate::page::composer::{ComposerChips, ComposerMenuSet, use_project_picking};
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
use vmux_ui::components::composer::{PROMPT_INPUT_ID, PromptComposer, focus_prompt_end};
use vmux_ui::components::composer_bar::{ComposerBar, ComposerMenus, use_composer_menu};
use vmux_ui::components::icon::Icon;
use vmux_ui::components::prompt_box::{PromptBox, PromptPopup, PromptPopupPlacement};
use vmux_ui::components::prompt_media_options::PromptMediaOptions;
use vmux_ui::hooks::{MenuDirection, send, use_key_claim, use_listener};
use vmux_ui::i18n::translate;
use vmux_ui::launcher::palette::{
    PaletteGlyph, PaletteRows, PaletteState, PaletteSurface, Submission,
};
use vmux_ui::launcher::row::ResultRow;
use vmux_ui::launcher::style::{
    command_bar_input_class, command_bar_input_row_class, command_bar_input_wrap_class,
    result_list_class,
};
use vmux_ui::scroll::ScrollIntoView;

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

    let model_menu_sel = use_signal(|| 0usize);
    let mut picking = use_project_picking();
    let _project_branches =
        use_listener::<StartProjectBranches, _>(START_PROJECT_BRANCHES_EVENT, move |incoming| {
            picking.remember(incoming.project, incoming.branches);
        });

    let rows = use_memo(move || PaletteRows::of(&state(), &feeds.draft(signals), surface));
    let mut palette_keys = PaletteKeys {
        rows,
        signals,
        on_dismiss,
    };
    let _key_listener =
        use_listener::<CommandBarKey, _>(COMMAND_BAR_KEY_EVENT, move |key| palette_keys.apply(key));

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
    let accent = palette.accent_agent.as_deref().map(agent_accent);
    let start_accent = accent.unwrap_or_else(|| agent_accent("vibe"));
    let start_prompt_attachments = media.composer_attachments();
    let start_action_enabled = !q.trim().is_empty() || !attachments.read().is_empty();
    let chips = ComposerChips::of(&composer, menu, model_menu_sel);
    let menus = ComposerMenuSet::of(&composer, signals, model_menu_sel, picking);
    let start_badges = rsx! {
        StartContextBadges {
            is_git_repo: composer.is_git_repo,
            is_worktree: composer.is_worktree,
            worktree_title: composer.worktree_title.clone(),
            uncommitted: composer.uncommitted,
            ahead: composer.ahead,
            cwd: composer.cwd.clone(),
        }
    };
    let start_status = rsx! {
        span { class: "flex h-7 shrink-0 items-center gap-1.5 rounded-lg px-2 text-[10px] text-muted-foreground",
            span { class: "h-1.5 w-1.5 rounded-full bg-success" }
            {translate("composer-ready")}
        }
    };
    let start_composer_footer = rsx! {
        ComposerBar {
            menu,
            agent: Some(chips.agent),
            model: chips.model,
            project: Some(chips.project),
            branch: chips.branch,
            badges: Some(start_badges),
            status: Some(start_status),
        }
    };
    let start_menus = rsx! {
        ComposerMenus {
            menu,
            placement: PromptPopupPlacement::Downward,
            agent: Some(menus.agent),
            model: Some(menus.model),
            project: Some(menus.project),
            branch: Some(menus.branch),
        }
    };

    let start_keydown = {
        let palette = palette.clone();
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

            if menu.opened().is_some()
                && (e.key() == Key::Escape || (ctrl && e.code() == Code::KeyC))
            {
                e.prevent_default();
                menu.close();
                return;
            }

            if media_menu_open && media.handle_key(&e, go_down, go_up, signals.query) {
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
        move |e: KeyboardEvent| {
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
                apply(palette.submit_modal(&attachments.peek()));
                return;
            }
            keys.on_keydown(&e, |_| false);
        }
    };
    let on_send = {
        let palette = palette.clone();
        move |_| apply(palette.submit_action(&attachments.peek()))
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
                    value: palette.display_text.clone(),
                    completion: ghost_text.clone(),
                    attachments: start_prompt_attachments,
                    show_examples: q.is_empty() && ghost_text.is_empty(),
                    placeholder: translate("command-composer-placeholder"),
                    accent_bg: start_accent.accent_bg.to_string(),
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
                        PaletteGlyphIcon { glyph: palette.glyph }
                        div { class: command_bar_input_wrap_class(),
                            if !ghost_text.is_empty() {
                                div {
                                    class: "pointer-events-none absolute inset-0 flex items-center",
                                    span { class: "invisible text-base", "{q}" }
                                    span { class: "text-base text-muted-foreground/40", "{ghost_text}" }
                                }
                            }
                            input {
                                id: "command-bar-input",
                                r#type: "text",
                                "data-ghost": "{ghost_text}",
                                class: command_bar_input_class(),
                                placeholder: palette.placeholder.clone(),
                                value: "{palette.display_text}",
                                autofocus: true,
                                oninput: move |event| signals.retype(event.value()),
                                onkeydown: modal_keydown,
                            }
                        }
                        BookmarkButton {}
                    }
                }
            }
            if menu.opened().is_none() && media_menu_open {
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
            if menu.opened().is_none() && !media_menu_open && !palette.rows.is_empty() {
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
fn StartContextBadges(
    is_git_repo: bool,
    is_worktree: bool,
    worktree_title: String,
    uncommitted: u32,
    ahead: u32,
    cwd: String,
) -> Element {
    rsx! {
        if is_git_repo {
            if is_worktree {
                span {
                    class: "flex h-7 shrink-0 items-center gap-1 rounded-lg bg-violet-500/[0.08] px-2 text-[10px] font-medium text-violet-600 ring-1 ring-inset ring-violet-500/15 dark:text-violet-300",
                    title: "{worktree_title}",
                    {translate("composer-worktree")}
                }
            }
            if uncommitted > 0 {
                span { class: "shrink-0 font-mono text-[10px] text-amber-500", title: translate("composer-uncommitted-changes"), "\u{25cf} {uncommitted}" }
            }
            if ahead > 0 {
                span { class: "shrink-0 font-mono text-[10px] text-sky-500", title: translate("composer-commits-ahead"), "\u{2191}{ahead}" }
            }
        } else if !cwd.is_empty() {
            span { class: "h-7 shrink-0 content-center rounded-lg px-2 text-[10px] text-muted-foreground/70", {translate("composer-no-git")} }
        }
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
