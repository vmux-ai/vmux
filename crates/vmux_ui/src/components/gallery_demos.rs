//! Full scrollable gallery: one demo per widget under [`vmux_ui::components`].

use std::collections::HashSet;

use dioxus::prelude::*;
use dioxus::signals::ReadSignal;
use dioxus_primitives::checkbox::CheckboxState;
use time::{Date, UtcDateTime};
use crate::components::{
    accordion::{Accordion, AccordionContent, AccordionItem, AccordionTrigger},
    alert_dialog::{
        AlertDialogAction, AlertDialogActions, AlertDialogCancel, AlertDialogContent,
        AlertDialogDescription, AlertDialogRoot, AlertDialogTitle,
    },
    aspect_ratio::AspectRatio,
    avatar::{Avatar, AvatarFallback, AvatarImage},
    badge::{Badge, BadgeVariant},
    button::{Button, ButtonVariant},
    calendar::{
        Calendar, CalendarGrid, CalendarHeader, CalendarMonthTitle, CalendarNavigation,
        CalendarNextMonthButton, CalendarPreviousMonthButton, CalendarView,
    },
    card::{Card, CardContent, CardDescription, CardFooter, CardHeader, CardTitle},
    checkbox::Checkbox,
    collapsible::{Collapsible, CollapsibleContent, CollapsibleTrigger},
    context_menu::{ContextMenu, ContextMenuContent, ContextMenuItem, ContextMenuTrigger},
    date_picker::{DatePicker, DatePickerInput},
    dialog::{DialogContent, DialogDescription, DialogRoot, DialogTitle},
    drag_and_drop_list::DragAndDropList,
    dropdown_menu::{DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger},
    hover_card::{HoverCard, HoverCardContent, HoverCardTrigger},
    icon::{Icon, ViewBox},
    input::Input,
    label::Label,
    menubar::{Menubar, MenubarContent, MenubarItem, MenubarMenu, MenubarTrigger},
    pagination::{
        Pagination, PaginationContent, PaginationEllipsis, PaginationItem, PaginationLink,
        PaginationNext, PaginationPrevious,
    },
    popover::{PopoverContent, PopoverRoot, PopoverTrigger},
    progress::{Progress, ProgressIndicator},
    radio_group::{RadioGroup, RadioItem},
    scroll_area::ScrollArea,
    select::{
        Select, SelectGroup, SelectItemIndicator, SelectList, SelectOption, SelectTrigger,
        SelectValue,
    },
    separator::Separator,
    sheet::{Sheet, SheetContent, SheetDescription, SheetHeader, SheetSide, SheetTitle},
    sidebar::{
        Sidebar, SidebarContent, SidebarInset, SidebarMenu, SidebarMenuButton, SidebarMenuItem,
        SidebarProvider, SidebarTrigger,
    },
    skeleton::Skeleton,
    slider::{Slider, SliderRange, SliderThumb, SliderTrack},
    switch::{Switch, SwitchThumb},
    tabs::{TabContent, TabList, TabTrigger, Tabs, TabsVariant},
    textarea::Textarea,
    toast::ToastProvider,
    toggle::Toggle,
    toggle_group::{ToggleGroup, ToggleItem},
    toolbar::{Toolbar, ToolbarButton, ToolbarGroup, ToolbarSeparator},
    tooltip::{Tooltip, TooltipContent, TooltipTrigger},
    virtual_list::VirtualList,
};

#[component]
pub fn GalleryDemos() -> Element {
    let dlg_id = use_signal(|| None::<String>);
    let mut dlg_open = use_signal(|| Some(false));
    let alert_id = use_signal(|| None::<String>);
    let mut alert_open = use_signal(|| Some(false));
    let mut alert_close_cancel = alert_open.clone();
    let mut alert_close_ok = alert_open.clone();
    let sheet_id = use_signal(|| None::<String>);
    let mut sheet_open = use_signal(|| Some(false));

    let mut tabs_val = use_signal(|| Some("t1".to_string()));
    let tabs_disabled = use_signal(|| false);
    let tabs_horizontal = use_signal(|| true);

    let mut selected_date = use_signal(|| None::<Date>);
    let mut view_date = use_signal(|| UtcDateTime::now().date());
    let mut picker_date = use_signal(|| None::<Date>);

    let mut radio_sel = use_signal(|| Some("r1".to_string()));
    let mut select_val = use_signal(|| Some(Some("apple".to_string())));
    let mut toggle_pressed = use_signal(|| Some(HashSet::from([0usize])));
    let tabs_roving = use_signal(|| true);

    let dlg_modal = use_signal(|| true);
    let opt_apple = use_signal(|| "apple".to_string());
    let opt_pear = use_signal(|| "pear".to_string());
    let vl_count = use_signal(|| 50usize);
    let vl_buffer = use_signal(|| 5usize);

    let dd_menu_a = use_signal(|| "a".to_string());
    let ctx_menu_x = use_signal(|| "x".to_string());

    rsx! {
        main { class: "mx-auto max-w-3xl space-y-10 px-5 py-8 pb-24",
            p { class: "text-[13px] text-muted-foreground",
                "Scroll to browse every vendored DioxusLabs widget in this crate."
            }

            section { class: "space-y-3",
                div { class: "flex items-center gap-3 text-left",
                    span { class: "text-[13px] text-foreground/90", "Typography" }
                    span { class: "h-px min-w-0 flex-1 bg-gradient-to-r from-border to-transparent" }
                }
                div { class: "rounded-xl border border-border bg-muted/30 p-4 text-ui leading-relaxed text-foreground",
                    div { class: "flex min-w-0 flex-col items-stretch gap-2",
                        span { class: "text-muted-foreground", "Muted" }
                        span { class: "text-primary", "Accent" }
                    }
                }
            }

            section { class: "space-y-3",
                div { class: "flex items-center gap-3 text-left",
                    span { class: "text-[13px] text-foreground/90", "Layout primitives" }
                    span { class: "h-px min-w-0 flex-1 bg-gradient-to-r from-border to-transparent" }
                }
                div { class: "rounded-xl border border-border bg-muted/30 p-4 text-ui leading-relaxed text-foreground",
                    div { class: "flex min-w-0 flex-row flex-nowrap items-center gap-1 flex-wrap items-center gap-2",
                        span { class: "text-foreground/90", "left" }
                        span { class: "inline-flex shrink-0 items-center font-bold text-tmux-dim", "|" }
                        div {
                            class: "flex flex-col items-center justify-center gap-3 rounded-2xl border border-dashed border-border bg-muted/20 px-6 py-14 text-center",
                            aria_label: "Demo",
                            span { class: "text-[11px] text-foreground/90", "Panel" }
                        }
                    }
                    Label { html_for: "gal-in", class: "sr-only", "Demo" }
                    div { class: "relative",
                        span { class: "pointer-events-none absolute inset-y-0 left-0 z-[1] flex w-9 shrink-0 items-center justify-center text-muted-foreground", aria_hidden: true,
                            Icon { view_box: ViewBox::new(0, 0, 24, 24), stroke_width: 2., class: "h-[14px] w-[14px]",
                                circle { cx: 11, cy: 11, r: 8 }
                                path { d: "m21 21-4.3-4.3" }
                            }
                        }
                        input { id: "gal-in", class: "w-full rounded-lg border border-border bg-muted/40 py-2 pl-9 pr-3 text-ui text-foreground outline-none placeholder:text-muted-foreground",
                            r#type: "text", placeholder: "Search…" }
                    }
                }
            }

            section { class: "space-y-3",
                div { class: "flex items-center gap-3 text-left",
                    span { class: "text-[13px] text-foreground/90", "Button & Badge" }
                    span { class: "h-px min-w-0 flex-1 bg-gradient-to-r from-border to-transparent" }
                }
                div { class: "rounded-xl border border-border bg-muted/30 p-4 text-ui leading-relaxed text-foreground",
                    div { class: "flex min-w-0 flex-row flex-nowrap items-center gap-1 flex-wrap gap-2",
                        Button { variant: ButtonVariant::Primary, "Primary" }
                        Button { variant: ButtonVariant::Secondary, "Secondary" }
                        Badge { variant: BadgeVariant::Primary, "Badge" }
                    }
                }
            }

            section { class: "space-y-3",
                div { class: "flex items-center gap-3 text-left",
                    span { class: "text-[13px] text-foreground/90", "Avatar & AspectRatio" }
                    span { class: "h-px min-w-0 flex-1 bg-gradient-to-r from-border to-transparent" }
                }
                div { class: "rounded-xl border border-border bg-muted/30 p-4 text-ui leading-relaxed text-foreground",
                    div { class: "flex min-w-0 flex-row flex-nowrap items-center gap-4",
                        Avatar {
                            AvatarImage { src: "", alt: "" }
                            AvatarFallback { "VX" }
                        }
                        AspectRatio { ratio: 16.0 / 9.0,
                            div { class: "flex h-full w-full items-center justify-center bg-muted", "16:9" }
                        }
                    }
                }
            }

            section { class: "space-y-3",
                div { class: "flex items-center gap-3 text-left",
                    span { class: "text-[13px] text-foreground/90", "Card" }
                    span { class: "h-px min-w-0 flex-1 bg-gradient-to-r from-border to-transparent" }
                }
                div { class: "rounded-xl border border-border bg-muted/30 p-4 text-ui leading-relaxed text-foreground",
                    Card {
                        CardHeader {
                            CardTitle { "Card title" }
                            CardDescription { "Description" }
                        }
                        CardContent { p { "Card body" } }
                        CardFooter { Button { variant: ButtonVariant::Primary, "Action" } }
                    }
                }
            }

            section { class: "space-y-3",
                div { class: "flex items-center gap-3 text-left",
                    span { class: "text-[13px] text-foreground/90", "Input & Textarea" }
                    span { class: "h-px min-w-0 flex-1 bg-gradient-to-r from-border to-transparent" }
                }
                div { class: "rounded-xl border border-border bg-muted/30 p-4 text-ui leading-relaxed text-foreground space-y-2",
                    Input { attributes: vec![], placeholder: None::<String>, children: rsx! {} }
                    Textarea { attributes: vec![], placeholder: None::<String>, children: rsx! {} }
                }
            }

            section { class: "space-y-3",
                div { class: "flex items-center gap-3 text-left",
                    span { class: "text-[13px] text-foreground/90", "Checkbox & Switch & Radio" }
                    span { class: "h-px min-w-0 flex-1 bg-gradient-to-r from-border to-transparent" }
                }
                div { class: "rounded-xl border border-border bg-muted/30 p-4 text-ui leading-relaxed text-foreground",
                    div { class: "flex min-w-0 flex-col items-stretch gap-3",
                        Checkbox {
                            default_checked: CheckboxState::Unchecked,
                            on_checked_change: Callback::new(|_| {}),
                            attributes: vec![], children: rsx! { "Check" }
                        }
                        Switch {
                            attributes: vec![], children: rsx! { SwitchThumb {} }
                        }
                        RadioGroup {
                            value: radio_sel(),
                            on_value_change: Callback::new(move |v| radio_sel.set(Some(v))),
                            attributes: vec![],
                            RadioItem { value: "r1", index: 0usize, attributes: vec![], "One" }
                            RadioItem { value: "r2", index: 1usize, attributes: vec![], "Two" }
                        }
                        Toggle { attributes: vec![], children: rsx! { "Toggle" } }
                    }
                }
            }

            section { class: "space-y-3",
                div { class: "flex items-center gap-3 text-left",
                    span { class: "text-[13px] text-foreground/90", "ToggleGroup" }
                    span { class: "h-px min-w-0 flex-1 bg-gradient-to-r from-border to-transparent" }
                }
                div { class: "rounded-xl border border-border bg-muted/30 p-4 text-ui leading-relaxed text-foreground",
                    ToggleGroup {
                        pressed: Into::<ReadSignal<Option<HashSet<usize>>>>::into(toggle_pressed),
                        on_pressed_change: Callback::new(move |s| toggle_pressed.set(Some(s))),
                        attributes: vec![],
                        ToggleItem { index: 0usize, attributes: vec![], "A" }
                        ToggleItem { index: 1usize, attributes: vec![], "B" }
                    }
                }
            }

            section { class: "space-y-3",
                div { class: "flex items-center gap-3 text-left",
                    span { class: "text-[13px] text-foreground/90", "Slider & Progress" }
                    span { class: "h-px min-w-0 flex-1 bg-gradient-to-r from-border to-transparent" }
                }
                div { class: "rounded-xl border border-border bg-muted/30 p-4 text-ui leading-relaxed text-foreground",
                    Slider { attributes: vec![], SliderTrack {}, SliderRange {}, SliderThumb {} }
                    Progress { attributes: vec![], value: Some(0.4), max: 1.0, ProgressIndicator {} }
                }
            }

            section { class: "space-y-3",
                div { class: "flex items-center gap-3 text-left",
                    span { class: "text-[13px] text-foreground/90", "Separator & Skeleton" }
                    span { class: "h-px min-w-0 flex-1 bg-gradient-to-r from-border to-transparent" }
                }
                div { class: "rounded-xl border border-border bg-muted/30 p-4 text-ui leading-relaxed text-foreground",
                    Separator { decorative: true, horizontal: false, attributes: vec![], children: rsx! {} }
                    Skeleton { height: Some("3rem"), width: Some("100%"), attributes: vec![] }
                }
            }

            section { class: "space-y-3",
                div { class: "flex items-center gap-3 text-left",
                    span { class: "text-[13px] text-foreground/90", "ScrollArea" }
                    span { class: "h-px min-w-0 flex-1 bg-gradient-to-r from-border to-transparent" }
                }
                div { class: "rounded-xl border border-border bg-muted/30 p-4 text-ui leading-relaxed text-foreground",
                    ScrollArea { attributes: vec![],
                        for i in 0..12 { p { key: "{i}", "Line {i}" } }
                    }
                }
            }

            section { class: "space-y-3",
                div { class: "flex items-center gap-3 text-left",
                    span { class: "text-[13px] text-foreground/90", "Tabs" }
                    span { class: "h-px min-w-0 flex-1 bg-gradient-to-r from-border to-transparent" }
                }
                div { class: "rounded-xl border border-border bg-muted/30 p-4 text-ui leading-relaxed text-foreground",
                    Tabs {
                        value: Into::<ReadSignal<Option<String>>>::into(tabs_val),
                        default_value: "t1".to_string(),
                        on_value_change: Callback::new(move |s| tabs_val.set(Some(s))),
                        disabled: Into::<ReadSignal<bool>>::into(tabs_disabled),
                        horizontal: Into::<ReadSignal<bool>>::into(tabs_horizontal),
                        roving_loop: Into::<ReadSignal<bool>>::into(tabs_roving),
                        variant: TabsVariant::Default,
                        attributes: vec![],
                        TabList { attributes: vec![],
                            TabTrigger { value: "t1", index: 0usize, attributes: vec![], "One" }
                            TabTrigger { value: "t2", index: 1usize, attributes: vec![], "Two" }
                        }
                        TabContent { value: "t1", index: 0usize, attributes: vec![], class: None, "Tab panel 1" }
                        TabContent { value: "t2", index: 1usize, attributes: vec![], class: None, "Tab panel 2" }
                    }
                }
            }

            section { class: "space-y-3",
                div { class: "flex items-center gap-3 text-left",
                    span { class: "text-[13px] text-foreground/90", "Accordion & Collapsible" }
                    span { class: "h-px min-w-0 flex-1 bg-gradient-to-r from-border to-transparent" }
                }
                div { class: "rounded-xl border border-border bg-muted/30 p-4 text-ui leading-relaxed text-foreground",
                    Accordion { attributes: vec![],
                        AccordionItem { index: 0usize, attributes: vec![],
                            AccordionTrigger { attributes: vec![], "Item A" }
                            AccordionContent { attributes: vec![], "Content A" }
                        }
                    }
                    Collapsible { attributes: vec![],
                        CollapsibleTrigger { attributes: vec![], "More" }
                        CollapsibleContent { attributes: vec![], "Hidden text" }
                    }
                }
            }

            section { class: "space-y-3",
                div { class: "flex items-center gap-3 text-left",
                    span { class: "text-[13px] text-foreground/90", "Dialog, AlertDialog, Sheet" }
                    span { class: "h-px min-w-0 flex-1 bg-gradient-to-r from-border to-transparent" }
                }
                div { class: "rounded-xl border border-border bg-muted/30 p-4 text-ui leading-relaxed text-foreground",
                    div { class: "flex min-w-0 flex-row flex-nowrap items-center gap-1 flex-wrap gap-2",
                        Button { variant: ButtonVariant::Outline, onclick: move |_| dlg_open.set(Some(true)), "Dialog" }
                        Button { variant: ButtonVariant::Outline, onclick: move |_| alert_open.set(Some(true)), "Alert" }
                        Button { variant: ButtonVariant::Outline, onclick: move |_| sheet_open.set(Some(true)), "Sheet" }
                    }
                    DialogRoot {
                        id: Into::<ReadSignal<Option<String>>>::into(dlg_id),
                        open: Into::<ReadSignal<Option<bool>>>::into(dlg_open),
                        on_open_change: Callback::new(move |o| dlg_open.set(Some(o))),
                        default_open: false,
                        is_modal: Into::<ReadSignal<bool>>::into(dlg_modal),
                        attributes: vec![],
                        DialogContent { attributes: vec![],
                            DialogTitle { attributes: vec![], "Dialog" }
                            DialogDescription { attributes: vec![], "Body" }
                            Button { variant: ButtonVariant::Primary, onclick: move |_| dlg_open.set(Some(false)), "Close" }
                        }
                    }
                    AlertDialogRoot {
                        id: Into::<ReadSignal<Option<String>>>::into(alert_id),
                        open: Into::<ReadSignal<Option<bool>>>::into(alert_open),
                        on_open_change: Callback::new(move |o| alert_open.set(Some(o))),
                        default_open: false,
                        attributes: vec![],
                        AlertDialogContent { attributes: vec![],
                            AlertDialogTitle { attributes: vec![], "Confirm" }
                            AlertDialogDescription { attributes: vec![], "Proceed?" }
                            AlertDialogActions { attributes: vec![],
                                AlertDialogCancel { attributes: vec![], on_click: Some(EventHandler::new(move |_| {
                                    alert_close_cancel.set(Some(false));
                                })), "Cancel" }
                                AlertDialogAction { attributes: vec![], on_click: Some(EventHandler::new(move |_| {
                                    alert_close_ok.set(Some(false));
                                })), "OK" }
                            }
                        }
                    }
                    Sheet {
                        id: Into::<ReadSignal<Option<String>>>::into(sheet_id),
                        open: Into::<ReadSignal<Option<bool>>>::into(sheet_open),
                        on_open_change: Callback::new(move |o| sheet_open.set(Some(o))),
                        default_open: false,
                        is_modal: true,
                        attributes: vec![],
                        SheetContent { side: SheetSide::Right, attributes: vec![],
                            SheetHeader { attributes: vec![],
                                SheetTitle { attributes: vec![], "Sheet" }
                                SheetDescription { attributes: vec![], "Side panel" }
                            }
                            Button { variant: ButtonVariant::Primary, onclick: move |_| sheet_open.set(Some(false)), "Close" }
                        }
                    }
                }
            }

            section { class: "space-y-3",
                div { class: "flex items-center gap-3 text-left",
                    span { class: "text-[13px] text-foreground/90", "Popover, Tooltip, HoverCard" }
                    span { class: "h-px min-w-0 flex-1 bg-gradient-to-r from-border to-transparent" }
                }
                div { class: "rounded-xl border border-border bg-muted/30 p-4 text-ui leading-relaxed text-foreground",
                    div { class: "flex min-w-0 flex-row flex-nowrap items-center gap-1 flex-wrap items-center gap-2",
                        PopoverRoot { default_open: false, attributes: vec![],
                            PopoverTrigger { attributes: vec![], Button { variant: ButtonVariant::Outline, "Popover" } }
                            PopoverContent { attributes: vec![], "Popover content" }
                        }
                        Tooltip { attributes: vec![],
                            TooltipTrigger { attributes: vec![], "Hover" }
                            TooltipContent { attributes: vec![], "Tip" }
                        }
                        HoverCard { attributes: vec![],
                            HoverCardTrigger { attributes: vec![], "Hover card" }
                            HoverCardContent { attributes: vec![], "Content" }
                        }
                    }
                }
            }

            section { class: "space-y-3",
                div { class: "flex items-center gap-3 text-left",
                    span { class: "text-[13px] text-foreground/90", "DropdownMenu & ContextMenu" }
                    span { class: "h-px min-w-0 flex-1 bg-gradient-to-r from-border to-transparent" }
                }
                div { class: "rounded-xl border border-border bg-muted/30 p-4 text-ui leading-relaxed text-foreground",
                    DropdownMenu { attributes: vec![],
                        DropdownMenuTrigger { attributes: vec![], "Menu" }
                        DropdownMenuContent { attributes: vec![],
                            DropdownMenuItem::<String> {
                                index: 0usize,
                                value: Into::<ReadSignal<String>>::into(dd_menu_a),
                                on_select: move |_: String| {},
                                attributes: vec![],
                                "One"
                            }
                        }
                    }
                    ContextMenu { attributes: vec![],
                        ContextMenuTrigger { attributes: vec![], "Right‑click" }
                        ContextMenuContent { attributes: vec![],
                            ContextMenuItem {
                                index: 0usize,
                                value: Into::<ReadSignal<String>>::into(ctx_menu_x),
                                on_select: move |_: String| {},
                                attributes: vec![],
                                "Action"
                            }
                        }
                    }
                }
            }

            section { class: "space-y-3",
                div { class: "flex items-center gap-3 text-left",
                    span { class: "text-[13px] text-foreground/90", "Menubar" }
                    span { class: "h-px min-w-0 flex-1 bg-gradient-to-r from-border to-transparent" }
                }
                div { class: "rounded-xl border border-border bg-muted/30 p-4 text-ui leading-relaxed text-foreground",
                    Menubar { attributes: vec![],
                        MenubarMenu { index: 0usize, attributes: vec![],
                            MenubarTrigger { attributes: vec![], "File" }
                            MenubarContent { attributes: vec![],
                                MenubarItem {
                                    index: 0usize,
                                    value: "n".to_string(),
                                    on_select: move |_: String| {},
                                    attributes: vec![],
                                    "New"
                                }
                            }
                        }
                    }
                    span { class: "text-[11px] text-muted-foreground",
                        "Navbar requires a dioxus_router shell; use the upstream components preview for full nav demos." }
                }
            }

            section { class: "space-y-3",
                div { class: "flex items-center gap-3 text-left",
                    span { class: "text-[13px] text-foreground/90", "Pagination & Toolbar" }
                    span { class: "h-px min-w-0 flex-1 bg-gradient-to-r from-border to-transparent" }
                }
                div { class: "rounded-xl border border-border bg-muted/30 p-4 text-ui leading-relaxed text-foreground",
                    Pagination { attributes: vec![],
                        PaginationContent { attributes: vec![],
                            PaginationItem { attributes: vec![], PaginationPrevious { attributes: vec![] } }
                            PaginationItem { attributes: vec![],
                                PaginationLink { is_active: true, attributes: vec![], children: rsx! { "1" } }
                            }
                            PaginationItem { attributes: vec![], PaginationEllipsis { attributes: vec![] } }
                            PaginationItem { attributes: vec![], PaginationNext { attributes: vec![] } }
                        }
                    }
                    Toolbar { attributes: vec![],
                        ToolbarGroup { attributes: vec![],
                            ToolbarButton { index: 0usize, attributes: vec![], "Cut" }
                            ToolbarSeparator { attributes: vec![] }
                            ToolbarButton { index: 1usize, attributes: vec![], "Copy" }
                        }
                    }
                }
            }

            section { class: "space-y-3",
                div { class: "flex items-center gap-3 text-left",
                    span { class: "text-[13px] text-foreground/90", "Select" }
                    span { class: "h-px min-w-0 flex-1 bg-gradient-to-r from-border to-transparent" }
                }
                div { class: "rounded-xl border border-border bg-muted/30 p-4 text-ui leading-relaxed text-foreground",
                    Select::<String> {
                        value: Into::<ReadSignal<Option<Option<String>>>>::into(select_val),
                        on_value_change: Callback::new(move |v| select_val.set(Some(v))),
                        default_value: Some("apple".into()),
                        placeholder: Into::<ReadSignal<String>>::into(use_signal(|| "Pick…".to_string())),
                        attributes: vec![],
                        SelectTrigger { attributes: vec![], SelectValue { attributes: vec![] } }
                        SelectList { attributes: vec![],
                            SelectGroup { attributes: vec![],
                                SelectOption::<String> {
                                    value: Into::<ReadSignal<String>>::into(opt_apple),
                                    index: 0usize,
                                    text_value: None,
                                    attributes: vec![],
                                    "Apple" SelectItemIndicator {}
                                }
                                SelectOption::<String> {
                                    value: Into::<ReadSignal<String>>::into(opt_pear),
                                    index: 1usize,
                                    text_value: None,
                                    attributes: vec![],
                                    "Pear" SelectItemIndicator {}
                                }
                            }
                        }
                    }
                }
            }

            section { class: "space-y-3",
                div { class: "flex items-center gap-3 text-left",
                    span { class: "text-[13px] text-foreground/90", "Calendar & DatePicker" }
                    span { class: "h-px min-w-0 flex-1 bg-gradient-to-r from-border to-transparent" }
                }
                div { class: "rounded-xl border border-border bg-muted/30 p-4 text-ui leading-relaxed text-foreground",
                    Calendar {
                        selected_date: selected_date(),
                        on_date_change: move |d| selected_date.set(d),
                        view_date: view_date(),
                        on_view_change: move |d| view_date.set(d),
                        attributes: vec![],
                        CalendarView {
                            CalendarHeader {
                                CalendarNavigation {
                                    CalendarPreviousMonthButton { attributes: vec![] }
                                    CalendarMonthTitle { attributes: vec![] }
                                    CalendarNextMonthButton { attributes: vec![] }
                                }
                            }
                            CalendarGrid { attributes: vec![] }
                        }
                    }
                    DatePicker {
                        selected_date: picker_date(),
                        on_value_change: move |d| picker_date.set(d),
                        attributes: vec![],
                        DatePickerInput { attributes: vec![] }
                    }
                }
            }

            section { class: "space-y-3",
                div { class: "flex items-center gap-3 text-left",
                    span { class: "text-[13px] text-foreground/90", "DragAndDropList & VirtualList" }
                    span { class: "h-px min-w-0 flex-1 bg-gradient-to-r from-border to-transparent" }
                }
                div { class: "rounded-xl border border-border bg-muted/30 p-4 text-ui leading-relaxed text-foreground",
                    DragAndDropList {
                        items: vec![rsx! { "Alpha" }, rsx! { "Beta" }, rsx! { "Gamma" }],
                        is_removable: false,
                        aria_label: Some("Reorder".into()),
                        attributes: vec![],
                        children: rsx! {}
                    }
                    VirtualList {
                        count: Into::<ReadSignal<usize>>::into(vl_count),
                        buffer: Into::<ReadSignal<usize>>::into(vl_buffer),
                        estimate_size: Some(Callback::new(|_| 28u32)),
                        render_item: Callback::new(|i| rsx! { div { class: "px-2 py-1", "Row {i}" } }),
                        attributes: vec![],
                    }
                }
            }

            section { class: "space-y-3",
                div { class: "flex items-center gap-3 text-left",
                    span { class: "text-[13px] text-foreground/90", "ToastProvider" }
                    span { class: "h-px min-w-0 flex-1 bg-gradient-to-r from-border to-transparent" }
                }
                div { class: "rounded-xl border border-border bg-muted/30 p-4 text-ui leading-relaxed text-foreground",
                    ToastProvider {
                        children: rsx! {
                            span { class: "text-[11px] text-muted-foreground",
                                "Toast host mounted (use use_toast() from a child to show toasts)." }
                        }
                    }
                }
            }

            section { class: "space-y-3",
                div { class: "flex items-center gap-3 text-left",
                    span { class: "text-[13px] text-foreground/90", "Sidebar" }
                    span { class: "h-px min-w-0 flex-1 bg-gradient-to-r from-border to-transparent" }
                }
                div { class: "rounded-xl border border-border bg-muted/30 p-4 text-ui leading-relaxed text-foreground overflow-hidden rounded-lg border border-border",
                    SidebarProvider { default_open: true, attributes: vec![],
                        Sidebar { attributes: vec![],
                            SidebarContent { attributes: vec![],
                                SidebarMenu { attributes: vec![],
                                    SidebarMenuItem { attributes: vec![],
                                        SidebarMenuButton { attributes: vec![], "Item" }
                                    }
                                }
                            }
                            SidebarInset { attributes: vec![],
                                SidebarTrigger { attributes: vec![] }
                                div { class: "p-4", span { class: "text-foreground/90", "Main" } }
                            }
                        }
                    }
                }
            }
        }
    }
}
