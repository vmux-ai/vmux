pub fn result_item_class(is_selected: bool) -> &'static str {
    if is_selected {
        "flex w-full cursor-pointer items-center justify-between bg-sidebar-primary px-3 py-2 text-sidebar-primary-foreground"
    } else {
        "flex w-full cursor-pointer items-center justify-between px-3 py-2 hover:bg-white/5"
    }
}

pub fn command_bar_shell_class() -> &'static str {
    "relative flex w-full max-w-xl flex-col overflow-hidden rounded-2xl border border-white/20 bg-white/10 shadow-2xl"
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
    fn command_bar_shell_does_not_apply_backdrop_filter() {
        let class = command_bar_shell_class();

        assert!(!class.contains("backdrop-"));
    }

    #[test]
    fn command_bar_document_keeps_backdrop_transparent() {
        let css = include_str!("../assets/index.css");

        assert!(!css.contains("background-color: var(--chrome-surface)"));
    }
}
