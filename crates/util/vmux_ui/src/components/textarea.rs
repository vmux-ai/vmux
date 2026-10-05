use dioxus::prelude::*;
use dioxus_primitives::dioxus_attributes::attributes;
use dioxus_primitives::merge_attributes;

#[derive(Copy, Clone, PartialEq, Default)]
#[non_exhaustive]
pub enum TextareaVariant {
    #[default]
    Default,
    Fade,
    Outline,
    Ghost,
}

impl TextareaVariant {
    pub fn class(&self) -> &'static str {
        match self {
            TextareaVariant::Default => "default",
            TextareaVariant::Fade => "fade",
            TextareaVariant::Outline => "outline",
            TextareaVariant::Ghost => "ghost",
        }
    }

    fn tw_classes(self) -> &'static str {
        match self {
            TextareaVariant::Default => {
                "w-full min-h-16 box-border resize-y appearance-none rounded-lg border-0 px-3 py-2 font-inherit text-base leading-normal text-muted-foreground outline-none transition-[background-color,border-color,box-shadow] placeholder:text-muted-foreground disabled:cursor-not-allowed bg-background shadow-[inset_0_0_0_1px_var(--border)] hover:bg-accent hover:text-foreground focus:bg-accent focus:text-foreground dark:bg-card dark:shadow-[inset_0_0_0_1px_var(--primary)] dark:hover:bg-muted"
            }
            TextareaVariant::Fade => {
                "w-full min-h-16 box-border resize-y appearance-none rounded-lg border-0 px-3 py-2 font-inherit text-base leading-normal text-muted-foreground outline-none transition-[background-color,border-color,box-shadow] placeholder:text-muted-foreground disabled:cursor-not-allowed bg-background hover:bg-accent hover:text-foreground focus:bg-accent focus:text-foreground dark:bg-card dark:hover:bg-muted"
            }
            TextareaVariant::Outline => {
                "w-full min-h-16 box-border resize-y appearance-none rounded-lg border-0 px-3 py-2 font-inherit text-base leading-normal text-muted-foreground outline-none transition-[background-color,border-color,box-shadow] placeholder:text-muted-foreground disabled:cursor-not-allowed border border-border bg-background hover:border-primary focus:border-ring aria-invalid:border-destructive dark:bg-card"
            }
            TextareaVariant::Ghost => {
                "w-full min-h-16 box-border resize-y appearance-none rounded-lg border-0 px-3 py-2 font-inherit text-base leading-normal text-muted-foreground outline-none transition-[background-color,border-color,box-shadow] placeholder:text-muted-foreground disabled:cursor-not-allowed bg-transparent hover:bg-muted hover:text-foreground focus:border-ring"
            }
        }
    }
}

#[component]
pub fn Textarea(
    oninput: Option<EventHandler<FormEvent>>,
    onchange: Option<EventHandler<FormEvent>>,
    oninvalid: Option<EventHandler<FormEvent>>,
    onselect: Option<EventHandler<SelectionEvent>>,
    onselectionchange: Option<EventHandler<SelectionEvent>>,
    onfocus: Option<EventHandler<FocusEvent>>,
    onblur: Option<EventHandler<FocusEvent>>,
    onfocusin: Option<EventHandler<FocusEvent>>,
    onfocusout: Option<EventHandler<FocusEvent>>,
    onkeydown: Option<EventHandler<KeyboardEvent>>,
    onkeypress: Option<EventHandler<KeyboardEvent>>,
    onkeyup: Option<EventHandler<KeyboardEvent>>,
    oncompositionstart: Option<EventHandler<CompositionEvent>>,
    oncompositionupdate: Option<EventHandler<CompositionEvent>>,
    oncompositionend: Option<EventHandler<CompositionEvent>>,
    oncopy: Option<EventHandler<ClipboardEvent>>,
    oncut: Option<EventHandler<ClipboardEvent>>,
    onpaste: Option<EventHandler<ClipboardEvent>>,
    #[props(default)] variant: TextareaVariant,
    #[props(extends = GlobalAttributes)]
    #[props(extends = textarea)]
    attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    let base = attributes!(textarea {
        class: variant.tw_classes(),
        "data-slot": "textarea",
        "data-style": variant.class(),
    });
    let merged = merge_attributes(vec![base, attributes]);
    rsx! {
        textarea {
            oninput: move |event| _ = oninput.map(|callback| callback(event)),
            onchange: move |event| _ = onchange.map(|callback| callback(event)),
            oninvalid: move |event| _ = oninvalid.map(|callback| callback(event)),
            onselect: move |event| _ = onselect.map(|callback| callback(event)),
            onselectionchange: move |event| _ = onselectionchange.map(|callback| callback(event)),
            onfocus: move |event| _ = onfocus.map(|callback| callback(event)),
            onblur: move |event| _ = onblur.map(|callback| callback(event)),
            onfocusin: move |event| _ = onfocusin.map(|callback| callback(event)),
            onfocusout: move |event| _ = onfocusout.map(|callback| callback(event)),
            onkeydown: move |event| _ = onkeydown.map(|callback| callback(event)),
            onkeypress: move |event| _ = onkeypress.map(|callback| callback(event)),
            onkeyup: move |event| _ = onkeyup.map(|callback| callback(event)),
            oncompositionstart: move |event| _ = oncompositionstart.map(|callback| callback(event)),
            oncompositionupdate: move |event| _ = oncompositionupdate.map(|callback| callback(event)),
            oncompositionend: move |event| _ = oncompositionend.map(|callback| callback(event)),
            oncopy: move |event| _ = oncopy.map(|callback| callback(event)),
            oncut: move |event| _ = oncut.map(|callback| callback(event)),
            onpaste: move |event| _ = onpaste.map(|callback| callback(event)),
            ..merged,
            {children}
        }
    }
}
