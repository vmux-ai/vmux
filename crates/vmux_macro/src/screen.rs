use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{Error, Expr, ExprArray, ItemFn, LitStr, Path, Token};

pub struct Args {
    pub(crate) url: Option<LitStr>,
    pub(crate) route: Option<Path>,
    pub(crate) presentation: Option<syn::Ident>,
    pub(crate) detents: Option<ExprArray>,
    pub(crate) painted: Option<Expr>,
    pub(crate) served_from: Option<LitStr>,
    pub(crate) owning_subtree: bool,
}

impl Parse for Args {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut args = Self {
            url: None,
            route: None,
            presentation: None,
            detents: None,
            painted: None,
            served_from: None,
            owning_subtree: false,
        };
        if input.peek(LitStr) {
            args.url = Some(input.parse()?);
            if input.is_empty() {
                return Ok(args);
            }
            input.parse::<Token![,]>()?;
        }
        if input.peek(syn::Ident) && input.peek2(Token![::]) {
            args.route = Some(input.parse()?);
            if input.is_empty() {
                return Ok(args);
            }
            input.parse::<Token![,]>()?;
        }
        if input.is_empty() {
            return Ok(args);
        }
        for option in Punctuated::<Setting, Token![,]>::parse_terminated(input)? {
            match option {
                Setting::Painted(expr) => args.painted = Some(*expr),
                Setting::ServedFrom(url) => args.served_from = Some(url),
                Setting::OwningSubtree => args.owning_subtree = true,
                Setting::Presented(kind) => args.presentation = Some(kind),
                Setting::Detents(sizes) => args.detents = Some(sizes),
            }
        }
        Ok(args)
    }
}

enum Setting {
    Painted(Box<Expr>),
    ServedFrom(LitStr),
    OwningSubtree,
    Presented(syn::Ident),
    Detents(ExprArray),
}

impl Parse for Setting {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let name: syn::Ident = input.parse()?;
        match name.to_string().as_str() {
            "owning_subtree" => Ok(Self::OwningSubtree),
            "background" => {
                input.parse::<Token![=]>()?;
                Ok(Self::Painted(Box::new(input.parse()?)))
            }
            "served_from" => {
                input.parse::<Token![=]>()?;
                Ok(Self::ServedFrom(input.parse()?))
            }
            "presentation" => {
                input.parse::<Token![=]>()?;
                Ok(Self::Presented(input.parse()?))
            }
            "detents" => {
                input.parse::<Token![=]>()?;
                Ok(Self::Detents(input.parse()?))
            }
            _ => Err(Error::new_spanned(
                name,
                "expected `background`, `served_from`, `owning_subtree`, `presentation` or `detents`",
            )),
        }
    }
}

pub fn expand(args: Args, component: ItemFn) -> TokenStream {
    let name = component.sig.ident.clone();
    let name = &name;
    let visibility = component.vis.clone();
    let visibility = &visibility;
    let spelled = name.to_string();
    let named = format_ident!("{}", screaming(&spelled));
    let url = match &args.url {
        Some(url) => quote! { #url },
        None => {
            let addressed = format!("vmux://{}/", kebab(&spelled));
            quote! { #addressed }
        }
    };

    let drawn_by = match &args.route {
        Some(_) => format_ident!("{}Body", name),
        None => name.clone(),
    };
    let mut page = quote! { ::vmux_native::NativePage::pane(#url, #drawn_by) };
    if let Some(url) = &args.served_from {
        page = quote! { #page.served_from(#url) };
    }
    if let Some(colour) = &args.painted {
        page = quote! { #page.background(#colour) };
    }
    if args.owning_subtree {
        page = quote! { #page.owning_subtree() };
    }

    let Some(route) = &args.route else {
        return quote! {
            #visibility static #named: ::vmux_native::NativePage = #page;

            #component
        };
    };
    let mut owner = route.clone();
    if owner.segments.len() < 2 {
        return Error::new_spanned(
            route,
            "a screen names a route variant, like `PageName::Card`",
        )
        .to_compile_error();
    }
    owner.segments.pop();
    let owner = owner
        .segments
        .into_pairs()
        .map(|pair| pair.into_value())
        .collect::<Punctuated<_, Token![::]>>();
    let owner = syn::Path {
        leading_colon: None,
        segments: owner,
    };
    let drawn = format_ident!("{}_PAGE", named);
    let body = format_ident!("{}Body", name);
    let presentation = match &args.presentation {
        Some(kind) => quote! { ::vmux_mobile::nav::Presentation::#kind },
        None => quote! { ::vmux_mobile::nav::Presentation::Card },
    };
    let detents = match &args.detents {
        Some(sizes) => quote! { &#sizes },
        None => quote! { &[] },
    };
    let mut drawing = component;
    drawing.sig.ident = body.clone();
    let page = quote! { #page };
    quote! {
        #visibility static #drawn: ::vmux_native::NativePage = #page;

        #visibility static #named: ::vmux_mobile::nav::ScreenPage<#owner> =
            ::vmux_mobile::nav::ScreenPage {
                page: &#drawn,
                name: #route,
                presentation: #presentation,
                detents: #detents,
            };

        #[::dioxus::prelude::component]
        #visibility fn #name() -> ::dioxus::prelude::Element {
            ::dioxus::prelude::rsx! {
                vmux_mobile::Screen { page: &#named }
            }
        }

        #drawing
    }
}

fn kebab(name: &str) -> String {
    let mut out = String::new();
    for (at, letter) in name.char_indices() {
        if letter.is_uppercase() && at > 0 {
            out.push('-');
        }
        out.extend(letter.to_lowercase());
    }
    out
}

fn screaming(name: &str) -> String {
    let mut out = String::new();
    for (at, letter) in name.char_indices() {
        if letter.is_uppercase() && at > 0 {
            out.push('_');
        }
        out.extend(letter.to_uppercase());
    }
    out
}
