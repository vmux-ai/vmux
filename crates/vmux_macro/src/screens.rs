use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{Error, Expr, ExprArray, Ident, Item, ItemFn, ItemMod, LitStr, Token, Type};

use crate::screen;

pub fn expand(module: ItemMod) -> syn::Result<TokenStream> {
    let Some((_, items)) = module.content else {
        return Err(Error::new_spanned(
            module.ident,
            "screens are declared in the module they live in",
        ));
    };
    let route = format_ident!("{}", pascal(&module.ident.to_string()));
    let named = format_ident!("{}Name", route);
    let visibility = module.vis;

    let mut variants = Vec::new();
    let mut screens = Vec::new();
    let mut spare = Vec::new();
    for item in items {
        let Item::Fn(component) = item else {
            spare.push(item);
            continue;
        };
        let Some(marked) = marked(&component) else {
            spare.push(Item::Fn(component));
            continue;
        };
        let (options, component) = split(component, marked)?;
        let name = component.sig.ident.clone();
        variants.push(options.variant(&name));
        screens.push(options.screen(&named, &name, component)?);
    }
    if variants.is_empty() {
        return Err(Error::new_spanned(
            route,
            "a screens module holds at least one `#[screen]`",
        ));
    }

    Ok(quote! {
        #[derive(Clone, PartialEq, ::vmux_mobile::nav::Route)]
        #visibility enum #route {
            #(#variants,)*
        }

        #(#spare)*
        #(#screens)*
    })
}

fn marked(component: &ItemFn) -> Option<usize> {
    for (at, attribute) in component.attrs.iter().enumerate() {
        if attribute.path().is_ident("screen") {
            return Some(at);
        }
    }
    None
}

fn split(mut component: ItemFn, at: usize) -> syn::Result<(Options, ItemFn)> {
    let attribute = component.attrs.remove(at);
    let options = match &attribute.meta {
        syn::Meta::Path(_) => Options::default(),
        _ => attribute.parse_args::<Options>()?,
    };
    Ok((options, component))
}

#[derive(Default)]
struct Options {
    holds: Option<Type>,
    title: Option<LitStr>,
    blank: bool,
    presentation: Option<Ident>,
    detents: Option<ExprArray>,
    painted: Option<Expr>,
    served_from: Option<LitStr>,
    owning_subtree: bool,
}

impl Options {
    fn variant(&self, name: &Ident) -> TokenStream {
        let blank = if self.blank {
            quote! { #[blank] }
        } else {
            quote! {}
        };
        let title = match &self.title {
            Some(title) => quote! { #[route(#title)] },
            None => quote! {},
        };
        match &self.holds {
            Some(holds) => quote! { #blank #title #name(#holds) },
            None => quote! { #blank #title #name },
        }
    }

    fn screen(self, named: &Ident, name: &Ident, component: ItemFn) -> syn::Result<TokenStream> {
        let route = syn::parse_quote! { #named::#name };
        let args = screen::Args {
            url: None,
            route: Some(route),
            presentation: self.presentation,
            detents: self.detents,
            painted: self.painted,
            served_from: self.served_from,
            owning_subtree: self.owning_subtree,
        };
        Ok(screen::expand(args, component))
    }
}

impl Parse for Options {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut options = Self::default();
        for setting in Punctuated::<Setting, Token![,]>::parse_terminated(input)? {
            match setting {
                Setting::Holds(holds) => options.holds = Some(*holds),
                Setting::Title(title) => options.title = Some(title),
                Setting::Blank => options.blank = true,
                Setting::Presented(kind) => options.presentation = Some(kind),
                Setting::Detents(sizes) => options.detents = Some(sizes),
                Setting::Painted(colour) => options.painted = Some(*colour),
                Setting::ServedFrom(url) => options.served_from = Some(url),
                Setting::OwningSubtree => options.owning_subtree = true,
            }
        }
        Ok(options)
    }
}

enum Setting {
    Holds(Box<Type>),
    Title(LitStr),
    Blank,
    Presented(Ident),
    Detents(ExprArray),
    Painted(Box<Expr>),
    ServedFrom(LitStr),
    OwningSubtree,
}

impl Parse for Setting {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let name: Ident = input.parse()?;
        match name.to_string().as_str() {
            "blank" => Ok(Self::Blank),
            "owning_subtree" => Ok(Self::OwningSubtree),
            "holds" => {
                input.parse::<Token![=]>()?;
                Ok(Self::Holds(Box::new(input.parse()?)))
            }
            "title" => {
                input.parse::<Token![=]>()?;
                Ok(Self::Title(input.parse()?))
            }
            "presentation" => {
                input.parse::<Token![=]>()?;
                Ok(Self::Presented(input.parse()?))
            }
            "detents" => {
                input.parse::<Token![=]>()?;
                Ok(Self::Detents(input.parse()?))
            }
            "background" => {
                input.parse::<Token![=]>()?;
                Ok(Self::Painted(Box::new(input.parse()?)))
            }
            "served_from" => {
                input.parse::<Token![=]>()?;
                Ok(Self::ServedFrom(input.parse()?))
            }
            _ => Err(Error::new_spanned(
                name,
                "expected `holds`, `title`, `blank`, `presentation`, `detents`, `background`, \
                 `served_from` or `owning_subtree`",
            )),
        }
    }
}

fn pascal(name: &str) -> String {
    let mut out = String::new();
    let mut upper = true;
    for letter in name.chars() {
        if letter == '_' {
            upper = true;
            continue;
        }
        if upper {
            out.extend(letter.to_uppercase());
            upper = false;
            continue;
        }
        out.push(letter);
    }
    out
}
