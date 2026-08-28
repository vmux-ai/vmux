use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{Error, Expr, ItemFn, LitStr, Token};

pub struct Args {
    url: Option<LitStr>,
    painted: Option<Expr>,
    served_from: Option<LitStr>,
    owning_subtree: bool,
}

impl Parse for Args {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut args = Self {
            url: None,
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
        if input.is_empty() {
            return Ok(args);
        }
        for option in Punctuated::<Setting, Token![,]>::parse_terminated(input)? {
            match option {
                Setting::Painted(expr) => args.painted = Some(*expr),
                Setting::ServedFrom(url) => args.served_from = Some(url),
                Setting::OwningSubtree => args.owning_subtree = true,
            }
        }
        Ok(args)
    }
}

enum Setting {
    Painted(Box<Expr>),
    ServedFrom(LitStr),
    OwningSubtree,
}

impl Parse for Setting {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let name: syn::Ident = input.parse()?;
        match name.to_string().as_str() {
            "owning_subtree" => Ok(Self::OwningSubtree),
            "painted" => {
                input.parse::<Token![=]>()?;
                Ok(Self::Painted(Box::new(input.parse()?)))
            }
            "served_from" => {
                input.parse::<Token![=]>()?;
                Ok(Self::ServedFrom(input.parse()?))
            }
            _ => Err(Error::new_spanned(
                name,
                "expected `painted`, `served_from` or `owning_subtree`",
            )),
        }
    }
}

pub fn expand(args: Args, component: ItemFn) -> TokenStream {
    let name = &component.sig.ident;
    let visibility = &component.vis;
    let spelled = name.to_string();
    let named = format_ident!("{}", screaming(&spelled));
    let url = match &args.url {
        Some(url) => quote! { #url },
        None => {
            let addressed = format!("vmux://{}/", kebab(&spelled));
            quote! { #addressed }
        }
    };

    let mut page = quote! { ::vmux_native::NativePage::pane(#url, #name) };
    if let Some(url) = &args.served_from {
        page = quote! { #page.served_from(#url) };
    }
    if let Some(colour) = &args.painted {
        page = quote! { #page.painted(#colour) };
    }
    if args.owning_subtree {
        page = quote! { #page.owning_subtree() };
    }

    quote! {
        #visibility static #named: ::vmux_native::NativePage = #page;

        #component
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
