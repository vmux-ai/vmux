pub fn result_item_class(is_selected: bool) -> &'static str {
    if is_selected {
        "flex min-h-15 min-w-0 w-full cursor-pointer items-center justify-between overflow-hidden bg-cyan-400/12 px-3.5 py-2.5 text-foreground shadow-[inset_2px_0_0_0_rgb(34,211,238),0_0_18px_-4px_rgba(34,211,238,0.45)]"
    } else {
        "flex min-h-15 min-w-0 w-full cursor-pointer items-center justify-between overflow-hidden px-3.5 py-2.5 hover:bg-foreground/5"
    }
}

pub fn command_bar_root_class(native_windowed: bool) -> &'static str {
    if native_windowed {
        "flex w-full flex-col overflow-x-hidden"
    } else {
        "flex h-full w-full items-start justify-center overflow-x-hidden pt-[15%]"
    }
}

pub fn command_bar_shell_class(native_windowed: bool) -> &'static str {
    if native_windowed {
        "relative flex w-full flex-col overflow-hidden rounded-2xl border border-border bg-background shadow-2xl"
    } else {
        "relative flex w-full max-w-xl flex-col overflow-hidden rounded-2xl border border-border bg-background shadow-2xl"
    }
}

pub fn command_bar_input_row_class() -> &'static str {
    "flex w-full min-w-0 flex-1 items-center gap-2 overflow-hidden rounded-lg bg-foreground/5 px-3"
}

pub fn command_bar_input_wrap_class() -> &'static str {
    "relative min-w-0 flex-1 overflow-hidden"
}

pub fn command_bar_input_class() -> &'static str {
    "w-full min-w-0 cursor-text bg-transparent py-2.5 text-base text-foreground caret-foreground outline-none placeholder:text-muted-foreground"
}

pub fn result_list_class() -> &'static str {
    "max-h-80 overflow-x-hidden overflow-y-auto border-t border-border"
}

pub fn result_content_row_class() -> &'static str {
    "flex min-w-0 flex-1 items-start gap-2 overflow-hidden"
}

pub fn result_favicon_class() -> &'static str {
    "mt-0.5 h-4 w-4 shrink-0 rounded-sm object-contain"
}

pub fn result_leading_icon_class() -> &'static str {
    "mt-0.5 h-4 w-4 shrink-0 text-muted-foreground"
}

pub fn result_primary_text_class() -> &'static str {
    "min-w-0 truncate text-base leading-snug text-foreground"
}

pub fn result_secondary_text_class() -> &'static str {
    "min-w-0 truncate text-sm leading-snug text-muted-foreground"
}

pub fn result_terminal_path_class() -> &'static str {
    "ml-1 min-w-0 truncate text-sm text-muted-foreground"
}

pub fn result_history_url_class() -> &'static str {
    "ml-auto min-w-0 max-w-xs truncate text-sm text-muted-foreground"
}

pub fn result_trailing_slot_class() -> &'static str {
    "ml-3 flex h-5 w-24 shrink-0 items-center justify-end overflow-hidden text-right text-xs text-muted-foreground"
}

pub fn result_location_class() -> &'static str {
    "ml-3 min-w-0 max-w-[46%] shrink-0 truncate rounded-md bg-foreground/[0.055] px-2 py-1 text-right font-mono text-[11px] text-muted-foreground ring-1 ring-inset ring-foreground/[0.06]"
}

pub fn result_shortcut_badge_class() -> &'static str {
    "max-w-full truncate rounded bg-muted px-1.5 py-0.5 text-xs text-muted-foreground"
}

#[cfg(test)]
#[path = "style.test.rs"]
mod tests;
