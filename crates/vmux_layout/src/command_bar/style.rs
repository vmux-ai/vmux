pub fn result_item_class(is_selected: bool) -> &'static str {
    if is_selected {
        "flex min-w-0 w-full cursor-pointer items-center justify-between overflow-hidden bg-sidebar-primary px-3 py-2 text-sidebar-primary-foreground"
    } else {
        "flex min-w-0 w-full cursor-pointer items-center justify-between overflow-hidden px-3 py-2 hover:bg-white/5"
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
        "relative flex w-full flex-col overflow-hidden rounded-2xl border border-border bg-transparent shadow-2xl"
    } else {
        "relative flex w-full max-w-xl flex-col overflow-hidden rounded-2xl border border-border bg-background shadow-2xl"
    }
}

pub fn command_bar_input_row_class() -> &'static str {
    "flex min-w-0 items-center gap-2 overflow-hidden rounded-lg bg-white/5 px-3"
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
    "flex min-w-0 flex-1 items-center gap-2 overflow-hidden"
}

pub fn result_primary_text_class() -> &'static str {
    "min-w-0 truncate text-base text-foreground"
}

pub fn result_secondary_text_class() -> &'static str {
    "min-w-0 truncate text-sm text-muted-foreground"
}

pub fn result_terminal_path_class() -> &'static str {
    "ml-1 min-w-0 truncate text-sm text-muted-foreground"
}

pub fn result_history_url_class() -> &'static str {
    "ml-auto min-w-0 max-w-xs truncate text-sm text-muted-foreground"
}

pub fn result_trailing_slot_class() -> &'static str {
    "ml-3 flex h-6 w-24 shrink-0 items-center justify-end overflow-hidden text-right text-sm text-muted-foreground"
}

pub fn result_shortcut_badge_class() -> &'static str {
    "max-w-full truncate rounded bg-muted px-1.5 py-0.5 text-sm text-muted-foreground"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_result_item_uses_blue_full_row_background() {
        let class = result_item_class(true);

        assert!(class.contains("bg-sidebar-primary"));
        assert!(class.contains("text-sidebar-primary-foreground"));
        assert!(class.contains("w-full"));
        assert!(!class.contains("bg-white/10"));
    }

    #[test]
    fn selected_result_item_clips_long_content() {
        let class = result_item_class(true);

        assert!(class.contains("min-w-0"));
        assert!(class.contains("overflow-hidden"));
    }

    #[test]
    fn unselected_result_item_clips_long_content() {
        let class = result_item_class(false);

        assert!(class.contains("min-w-0"));
        assert!(class.contains("overflow-hidden"));
    }

    #[test]
    fn command_bar_root_disables_horizontal_scroll() {
        let class = command_bar_root_class(false);

        assert!(class.contains("overflow-x-hidden"));
    }

    #[test]
    fn command_bar_root_preserves_osr_positioning() {
        let class = command_bar_root_class(false);

        assert!(class.contains("h-full"));
        assert!(class.contains("pt-[15%]"));
    }

    #[test]
    fn command_bar_root_fits_native_view() {
        let class = command_bar_root_class(true);

        assert!(!class.contains("h-full"));
        assert!(!class.contains("pt-[15%]"));
    }

    #[test]
    fn command_bar_shell_does_not_apply_backdrop_filter() {
        let class = command_bar_shell_class(false);

        assert!(!class.contains("backdrop-"));
    }

    #[test]
    fn command_bar_document_keeps_backdrop_transparent() {
        let css = include_str!("../../../vmux_command/assets/index.css");

        assert!(!css.contains("background-color: var(--cef-surface)"));
    }

    #[test]
    fn command_bar_shell_uses_solid_background() {
        let class = command_bar_shell_class(false);

        assert!(class.contains("bg-background"));
        assert!(!class.contains("bg-white/10"));
    }

    #[test]
    fn command_bar_shell_preserves_osr_max_width() {
        let class = command_bar_shell_class(false);

        assert!(class.contains("max-w-xl"));
    }

    #[test]
    fn command_bar_shell_fills_native_view_width() {
        let class = command_bar_shell_class(true);

        assert!(!class.contains("max-w-xl"));
        assert!(class.contains("w-full"));
    }

    #[test]
    fn command_bar_native_shell_uses_transparent_background() {
        let class = command_bar_shell_class(true);

        assert!(class.contains("bg-transparent"));
        assert!(!class.contains("bg-background"));
    }

    #[test]
    fn command_bar_input_shrinks_inside_shell() {
        let row = command_bar_input_row_class();
        let wrap = command_bar_input_wrap_class();
        let input = command_bar_input_class();

        assert!(row.contains("min-w-0"));
        assert!(row.contains("overflow-hidden"));
        assert!(wrap.contains("min-w-0"));
        assert!(wrap.contains("overflow-hidden"));
        assert!(input.contains("min-w-0"));
    }

    #[test]
    fn command_bar_input_shows_text_cursor() {
        let input = command_bar_input_class();

        assert!(input.contains("cursor-text"));
        assert!(input.contains("caret-foreground"));
    }

    #[test]
    fn results_list_disables_horizontal_scroll() {
        let class = result_list_class();

        assert!(class.contains("overflow-x-hidden"));
    }

    #[test]
    fn result_text_rows_shrink_and_truncate() {
        let row = result_content_row_class();
        let primary = result_primary_text_class();
        let secondary = result_secondary_text_class();
        let terminal = result_terminal_path_class();
        let history = result_history_url_class();

        assert!(row.contains("min-w-0"));
        assert!(row.contains("overflow-hidden"));
        assert!(primary.contains("min-w-0"));
        assert!(primary.contains("truncate"));
        assert!(secondary.contains("min-w-0"));
        assert!(secondary.contains("truncate"));
        assert!(terminal.contains("min-w-0"));
        assert!(terminal.contains("truncate"));
        assert!(history.contains("min-w-0"));
        assert!(history.contains("truncate"));
    }

    #[test]
    fn result_trailing_slot_has_fixed_width_and_right_alignment() {
        let class = result_trailing_slot_class();

        assert!(class.contains("w-24"));
        assert!(class.contains("shrink-0"));
        assert!(class.contains("justify-end"));
        assert!(class.contains("text-right"));
        assert!(class.contains("overflow-hidden"));
    }

    #[test]
    fn result_shortcut_badge_truncates_inside_slot() {
        let class = result_shortcut_badge_class();

        assert!(class.contains("max-w-full"));
        assert!(class.contains("truncate"));
    }

    #[test]
    fn result_rows_reserve_aligned_trailing_slot() {
        let source = include_str!("page.rs");

        assert!(source.contains("result_trailing_slot_class()"));
        assert!(source.contains("result_shortcut_badge_class()"));
    }

    #[test]
    fn command_bar_document_disables_horizontal_overflow() {
        let css = include_str!("../../assets/index.css");

        assert!(css.contains("overflow-x-hidden"));
    }

    #[test]
    fn layout_css_keeps_glass_background_transparent() {
        let css = include_str!("../../assets/index.css");

        assert!(css.contains("--glass: transparent;"));
        assert!(!css.contains("--glass: oklch(0.36 0 0 / 0.82);"));
    }
}
