use dioxus::prelude::*;
use dioxus_primitives::dialog::{
    self, DialogCtx, DialogDescriptionProps, DialogRootProps, DialogTitleProps,
};
use dioxus_primitives::dioxus_attributes::attributes;
use dioxus_primitives::icon;

use crate::util::merge_class;

const SHEET_ROOT: &str = "group/sheet fixed inset-0 z-[1000] bg-scrim-strong opacity-0 will-change-opacity data-[state=closed]:pointer-events-none data-[state=closed]:animate-[sheet-scrim-out_300ms_ease-in_forwards] data-[state=open]:animate-[sheet-scrim-in_300ms_ease-out_forwards]";

const SHEET_PANEL_BASE: &str = "fixed z-[1001] flex flex-col gap-4 border-0 bg-background font-sans text-muted-foreground shadow-[0_4px_20px_rgb(0_0_0_/_20%)] will-change-transform";

const SHEET_CLOSE: &str = "cursor-pointer absolute right-4 top-4 flex size-6 items-center justify-center rounded border-0 bg-transparent p-0 text-primary transition-colors hover:text-muted-foreground";

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum SheetSide {
    Top,
    #[default]
    Right,
    Bottom,
    Left,
}

impl SheetSide {
    pub fn as_str(&self) -> &'static str {
        match self {
            SheetSide::Top => "top",
            SheetSide::Right => "right",
            SheetSide::Bottom => "bottom",
            SheetSide::Left => "left",
        }
    }

    fn panel_classes(self) -> &'static str {
        match self {
            SheetSide::Right => {
                "data-[side=right]:inset-y-0 data-[side=right]:right-0 data-[side=right]:w-3/4 data-[side=right]:max-w-sm data-[side=right]:border-l data-[side=right]:border-border data-[side=right]:translate-x-full group-data-[state=open]/sheet:data-[side=right]:translate-x-0 group-data-[state=open]/sheet:data-[side=right]:animate-[slide-in-right_500ms_ease-out_forwards] group-data-[state=closed]/sheet:data-[side=right]:animate-[slide-out-right_300ms_ease-in_forwards]"
            }
            SheetSide::Left => {
                "data-[side=left]:inset-y-0 data-[side=left]:left-0 data-[side=left]:w-3/4 data-[side=left]:max-w-sm data-[side=left]:border-r data-[side=left]:border-border data-[side=left]:-translate-x-full group-data-[state=open]/sheet:data-[side=left]:translate-x-0 group-data-[state=open]/sheet:data-[side=left]:animate-[slide-in-left_500ms_ease-out_forwards] group-data-[state=closed]/sheet:data-[side=left]:animate-[slide-out-left_300ms_ease-in_forwards]"
            }
            SheetSide::Top => {
                "data-[side=top]:inset-x-0 data-[side=top]:top-0 data-[side=top]:border-b data-[side=top]:border-border data-[side=top]:-translate-y-full group-data-[state=open]/sheet:data-[side=top]:translate-y-0 group-data-[state=open]/sheet:data-[side=top]:animate-[slide-in-top_500ms_ease-out_forwards] group-data-[state=closed]/sheet:data-[side=top]:animate-[slide-out-top_300ms_ease-in_forwards]"
            }
            SheetSide::Bottom => {
                "data-[side=bottom]:inset-x-0 data-[side=bottom]:bottom-0 data-[side=bottom]:border-t data-[side=bottom]:border-border data-[side=bottom]:translate-y-full group-data-[state=open]/sheet:data-[side=bottom]:translate-y-0 group-data-[state=open]/sheet:data-[side=bottom]:animate-[slide-in-bottom_500ms_ease-out_forwards] group-data-[state=closed]/sheet:data-[side=bottom]:animate-[slide-out-bottom_300ms_ease-in_forwards]"
            }
        }
    }
}

#[component]
pub fn Sheet(props: DialogRootProps) -> Element {
    rsx! {
        SheetRoot {
            id: props.id,
            is_modal: props.is_modal,
            open: props.open,
            default_open: props.default_open,
            on_open_change: props.on_open_change,
            attributes: props.attributes,
            {props.children}
        }
    }
}

#[component]
fn SheetRoot(props: DialogRootProps) -> Element {
    rsx! {
        dialog::DialogRoot {
            class: SHEET_ROOT,
            "data-slot": "sheet-root",
            id: props.id,
            is_modal: props.is_modal,
            open: props.open,
            default_open: props.default_open,
            on_open_change: props.on_open_change,
            attributes: props.attributes,
            {props.children}
        }
    }
}

#[component]
pub fn SheetContent(
    #[props(default = ReadSignal::new(Signal::new(None)))] id: ReadSignal<Option<String>>,
    #[props(default)] side: SheetSide,
    #[props(default)] class: Option<String>,
    #[props(extends = GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    let panel = format!(
        "{} {} {}",
        SHEET_PANEL_BASE,
        side.panel_classes(),
        class.as_deref().unwrap_or("")
    );
    rsx! {
        dialog::DialogContent {
            class: Some(panel.trim().to_string()),
            id,
            "data-slot": "sheet-content",
            "data-side": side.as_str(),
            attributes,
            {children}
            SheetClose {
                icon::Icon {
                    width: "20px",
                    height: "20px",
                    path { d: "M18 6 6 18" }
                    path { d: "m6 6 12 12" }
                }
            }
        }
    }
}

#[component]
pub fn SheetHeader(
    #[props(extends = GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    rsx! {
        div { class: "flex flex-col gap-1.5 p-4", "data-slot": "sheet-header", ..attributes, {children} }
    }
}

#[component]
pub fn SheetFooter(
    #[props(extends = GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    rsx! {
        div { class: "mt-auto flex flex-col gap-2 p-4", "data-slot": "sheet-footer", ..attributes, {children} }
    }
}

#[component]
pub fn SheetTitle(props: DialogTitleProps) -> Element {
    rsx! {
        dialog::DialogTitle {
            id: props.id,
            class: "m-0 text-lg font-semibold text-muted-foreground",
            "data-slot": "sheet-title",
            attributes: props.attributes,
            {props.children}
        }
    }
}

#[component]
pub fn SheetDescription(props: DialogDescriptionProps) -> Element {
    rsx! {
        dialog::DialogDescription {
            id: props.id,
            class: "m-0 text-sm text-muted-foreground",
            "data-slot": "sheet-description",
            attributes: props.attributes,
            {props.children}
        }
    }
}

#[component]
pub fn SheetClose(
    #[props(default)] class: Option<String>,
    #[props(extends = GlobalAttributes)] attributes: Vec<Attribute>,
    r#as: Option<Callback<Vec<Attribute>, Element>>,
    children: Element,
) -> Element {
    let ctx: DialogCtx = use_context();
    let cls = merge_class(SHEET_CLOSE, class.as_deref());

    let mut merged: Vec<Attribute> = attributes! {
        button {
            class: cls,
            onclick: move |_| {
                ctx.set_open(false);
            }
        }
    };
    merged.extend(attributes);

    if let Some(dynamic) = r#as {
        dynamic.call(merged)
    } else {
        rsx! {
            button { ..merged, {children} }
        }
    }
}
