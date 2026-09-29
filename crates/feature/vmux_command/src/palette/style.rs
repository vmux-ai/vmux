pub(crate) fn result_item_class(is_selected: bool) -> &'static str {
    if is_selected {
        "flex min-h-15 min-w-0 w-full cursor-pointer items-center justify-between overflow-hidden bg-primary/12 px-3.5 py-2.5 text-foreground shadow-[inset_2px_0_0_0_var(--primary),0_0_18px_-4px_color-mix(in_oklab,var(--primary)_45%,transparent)]"
    } else {
        "flex min-h-15 min-w-0 w-full cursor-pointer items-center justify-between overflow-hidden px-3.5 py-2.5 hover:bg-foreground/5"
    }
}

pub(crate) fn command_bar_root_class(native_windowed: bool) -> &'static str {
    if native_windowed {
        "flex w-full flex-col overflow-x-hidden"
    } else {
        "flex h-full w-full items-start justify-center overflow-x-hidden pt-[15%]"
    }
}

pub(crate) fn command_bar_shell_class(native_windowed: bool) -> &'static str {
    if native_windowed {
        "relative flex w-full flex-col overflow-hidden rounded-2xl border border-border bg-background shadow-2xl"
    } else {
        "relative flex w-full max-w-xl flex-col overflow-hidden rounded-2xl border border-border bg-background shadow-2xl"
    }
}

pub(crate) const COMMAND_BAR_INPUT_ROW_CLASS: &str =
    "flex w-full min-w-0 flex-1 items-center gap-2 overflow-hidden rounded-lg bg-foreground/5 px-3";
pub(crate) const COMMAND_BAR_INPUT_WRAP_CLASS: &str = "relative min-w-0 flex-1 overflow-hidden";

pub(crate) fn command_bar_input_class(row_overlaid: bool) -> &'static str {
    if row_overlaid {
        "w-full min-w-0 cursor-text bg-transparent py-2.5 text-base text-transparent caret-foreground outline-none placeholder:text-muted-foreground"
    } else {
        "w-full min-w-0 cursor-text bg-transparent py-2.5 text-base text-foreground caret-foreground outline-none placeholder:text-muted-foreground"
    }
}

pub(crate) const COMMAND_BAR_ROW_OVERLAY_CLASS: &str =
    "pointer-events-none absolute inset-0 flex items-center";
pub(crate) const RESULT_LIST_CLASS: &str =
    "max-h-80 overflow-x-hidden overflow-y-auto border-t border-border";
pub(crate) const RESULT_CONTENT_ROW_CLASS: &str =
    "flex min-w-0 flex-1 items-start gap-2 overflow-hidden";
pub(crate) const RESULT_FAVICON_CLASS: &str = "mt-0.5 h-4 w-4 shrink-0 rounded-sm object-contain";
pub(crate) const RESULT_LEADING_ICON_CLASS: &str = "mt-0.5 h-4 w-4 shrink-0 text-muted-foreground";
pub(crate) const RESULT_PRIMARY_TEXT_CLASS: &str =
    "min-w-0 truncate text-base leading-snug text-foreground";
pub(crate) const RESULT_SECONDARY_TEXT_CLASS: &str =
    "min-w-0 truncate text-sm leading-snug text-muted-foreground";
pub(crate) const RESULT_TERMINAL_PATH_CLASS: &str =
    "ml-1 min-w-0 truncate text-sm text-muted-foreground";
pub(crate) const RESULT_HISTORY_URL_CLASS: &str =
    "ml-auto min-w-0 max-w-xs truncate text-sm text-muted-foreground";
pub(crate) const RESULT_TRAILING_SLOT_CLASS: &str = "ml-3 flex h-5 w-24 shrink-0 items-center justify-end overflow-hidden text-right text-xs text-muted-foreground";
pub(crate) const RESULT_LOCATION_CLASS: &str = "ml-3 min-w-0 max-w-[46%] shrink-0 truncate rounded-md bg-foreground/[0.055] px-2 py-1 text-right font-mono text-[11px] text-muted-foreground ring-1 ring-inset ring-foreground/[0.06]";
pub(crate) const RESULT_SHORTCUT_BADGE_CLASS: &str =
    "max-w-full truncate rounded bg-muted px-1.5 py-0.5 text-xs text-muted-foreground";
