use heck::ToSnakeCase;
use proc_macro2::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Attribute, DeriveInput, Ident, LitInt, LitStr, Token, bracketed};

mod keyword {
    syn::custom_keyword!(any);
    syn::custom_keyword!(target);
    syn::custom_keyword!(targets);
    syn::custom_keyword!(version);
}

pub(crate) enum Direction {
    Host,
    Ui,
    Both,
}

enum Target {
    Any,
    Host(LitStr),
    Hosts(Vec<LitStr>),
}

struct Args {
    version: Option<LitInt>,
    target: Target,
}

impl Parse for Args {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut version: Option<LitInt> = None;
        let mut target: Option<Target> = None;

        while !input.is_empty() {
            if input.peek(keyword::version) {
                let key: keyword::version = input.parse()?;
                input.parse::<Token![=]>()?;
                if version.is_some() {
                    return Err(syn::Error::new_spanned(key, "duplicate version"));
                }
                version = Some(input.parse()?);
            } else if input.peek(keyword::target) {
                let key: keyword::target = input.parse()?;
                input.parse::<Token![=]>()?;
                if target.is_some() {
                    return Err(syn::Error::new_spanned(key, "duplicate target"));
                }
                target = if input.peek(keyword::any) {
                    input.parse::<keyword::any>()?;
                    Some(Target::Any)
                } else {
                    Some(Target::Host(input.parse()?))
                };
            } else if input.peek(keyword::targets) {
                let key: keyword::targets = input.parse()?;
                input.parse::<Token![=]>()?;
                if target.is_some() {
                    return Err(syn::Error::new_spanned(key, "duplicate target"));
                }
                let content;
                bracketed!(content in input);
                let values = content
                    .parse_terminated(|input| input.parse::<LitStr>(), Token![,])?
                    .into_iter()
                    .collect();
                target = Some(Target::Hosts(values));
            } else {
                let key: Ident = input.parse()?;
                return Err(syn::Error::new_spanned(key, "unknown bin event option"));
            }
            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }

        let target = target.ok_or_else(|| input.error("missing target"))?;
        Ok(Self { version, target })
    }
}

fn inferred_event_name(ident: &Ident) -> LitStr {
    let snake = ident.to_string().to_snake_case();
    let stem = snake
        .strip_suffix("_request")
        .or_else(|| snake.strip_suffix("_event"))
        .unwrap_or(&snake);
    LitStr::new(stem, ident.span())
}

pub(crate) fn expand(
    args: TokenStream,
    input: DeriveInput,
    direction: Direction,
) -> syn::Result<TokenStream> {
    let implementation = implementation(&input, direction, syn::parse2(args)?)?;

    Ok(quote! {
        #input
        #implementation
    })
}

fn implementation(
    input: &DeriveInput,
    direction: Direction,
    Args { version, target }: Args,
) -> syn::Result<TokenStream> {
    let ident = &input.ident;
    let name = inferred_event_name(ident);
    let generics = &input.generics;
    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();
    let version_value = version
        .as_ref()
        .map(LitInt::base10_parse::<u16>)
        .transpose()?
        .unwrap_or(1);
    let id = format!("{}@{version_value}", name.value());
    let id = LitStr::new(&id, name.span());
    let version = version
        .map(|version| quote! { const VERSION: u16 = #version; })
        .unwrap_or_default();
    let target = match target {
        Target::Any => quote! { ::vmux_api::BinEventTarget::Any },
        Target::Host(host) => quote! { ::vmux_api::BinEventTarget::Host(#host) },
        Target::Hosts(hosts) => quote! { ::vmux_api::BinEventTarget::Hosts(&[#(#hosts),*]) },
    };
    let direction_impl = match direction {
        Direction::Host => quote! {
            impl #impl_generics ::vmux_api::HostEvent for #ident #type_generics #where_clause {}
        },
        Direction::Ui => quote! {
            impl #impl_generics ::vmux_api::UiEvent for #ident #type_generics #where_clause {}
        },
        Direction::Both => quote! {
            impl #impl_generics ::vmux_api::HostEvent for #ident #type_generics #where_clause {}
            impl #impl_generics ::vmux_api::UiEvent for #ident #type_generics #where_clause {}
        },
    };

    Ok(quote! {
        impl #impl_generics ::vmux_api::BinEvent for #ident #type_generics #where_clause {
            const ID: &'static str = #id;
            const NAME: &'static str = #name;
            #version
            const TARGET: ::vmux_api::BinEventTarget = #target;
        }

        #direction_impl
    })
}

pub(crate) fn derive(input: DeriveInput, direction: Direction) -> syn::Result<TokenStream> {
    if let Some(args) = event_args(&input.attrs)? {
        return implementation(&input, direction, args);
    }
    let ident = &input.ident;
    let generics = &input.generics;
    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();
    let name = inferred_event_name(ident);
    let id = LitStr::new(&format!("{}@1", name.value()), ident.span());
    let direction_impl = match direction {
        Direction::Host => quote! {
            impl #impl_generics ::vmux_api::HostEvent for #ident #type_generics #where_clause {}
        },
        Direction::Ui => quote! {
            impl #impl_generics ::vmux_api::UiEvent for #ident #type_generics #where_clause {}
        },
        Direction::Both => quote! {
            impl #impl_generics ::vmux_api::HostEvent for #ident #type_generics #where_clause {}
            impl #impl_generics ::vmux_api::UiEvent for #ident #type_generics #where_clause {}
        },
    };

    Ok(quote! {
        impl #impl_generics ::vmux_api::BinEvent for #ident #type_generics #where_clause {
            const ID: &'static str = #id;
            const NAME: &'static str = #name;
            const TARGET: ::vmux_api::BinEventTarget =
                <Events as ::vmux_api::BinEventFamily>::TARGET;
        }

        #direction_impl
    })
}

fn event_args(attributes: &[Attribute]) -> syn::Result<Option<Args>> {
    let mut event = None;
    for attribute in attributes {
        if !attribute.path().is_ident("event") {
            continue;
        }
        if event.is_some() {
            return Err(syn::Error::new_spanned(
                attribute,
                "duplicate event attribute",
            ));
        }
        event = Some(attribute.parse_args()?);
    }
    Ok(event)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proc_macro2::Span;

    #[test]
    fn request_type_infers_name() {
        let ident = Ident::new("BookmarkMenuPinRequest", Span::call_site());
        let name = inferred_event_name(&ident);
        assert_eq!(name.value(), "bookmark_menu_pin");
    }

    #[test]
    fn event_type_infers_name() {
        let ident = Ident::new("EditorPageEvent", Span::call_site());
        let name = inferred_event_name(&ident);
        assert_eq!(name.value(), "editor_page");
    }
}
