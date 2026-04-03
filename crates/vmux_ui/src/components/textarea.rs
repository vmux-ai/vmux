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
            oninput: move |e| _ = oninput.map(|callback| callback(e)),
            onchange: move |e| _ = onchange.map(|callback| callback(e)),
            oninvalid: move |e| _ = oninvalid.map(|callback| callback(e)),
            onselect: move |e| _ = onselect.map(|callback| callback(e)),
            onselectionchange: move |e| _ = onselectionchange.map(|callback| callback(e)),
            onfocus: move |e| _ = onfocus.map(|callback| callback(e)),
            onblur: move |e| _ = onblur.map(|callback| callback(e)),
            onfocusin: move |e| _ = onfocusin.map(|callback| callback(e)),
            onfocusout: move |e| _ = onfocusout.map(|callback| callback(e)),
            onkeydown: move |e| _ = onkeydown.map(|callback| callback(e)),
            onkeypress: move |e| _ = onkeypress.map(|callback| callback(e)),
            onkeyup: move |e| _ = onkeyup.map(|callback| callback(e)),
            oncompositionstart: move |e| _ = oncompositionstart.map(|callback| callback(e)),
            oncompositionupdate: move |e| _ = oncompositionupdate.map(|callback| callback(e)),
            oncompositionend: move |e| _ = oncompositionend.map(|callback| callback(e)),
            oncopy: move |e| _ = oncopy.map(|callback| callback(e)),
            oncut: move |e| _ = oncut.map(|callback| callback(e)),
            onpaste: move |e| _ = onpaste.map(|callback| callback(e)),
            ..merged,
            {children}
        }
    }
}
