use heck::ToSnakeCase;
use proc_macro2::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{DeriveInput, Ident, LitInt, LitStr, Path, Token};

mod keyword {
    syn::custom_keyword!(version);
}

pub(crate) enum Direction {
    Host,
    Ui,
    Both,
    Agent,
}

struct Args {
    derives: Vec<Path>,
    version: Option<LitInt>,
}

impl Parse for Args {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut version: Option<LitInt> = None;
        let mut derives = Vec::new();

        while !input.is_empty() {
            if input.peek(keyword::version) {
                let key: keyword::version = input.parse()?;
                input.parse::<Token![=]>()?;
                if version.is_some() {
                    return Err(syn::Error::new_spanned(key, "duplicate version"));
                }
                version = Some(input.parse()?);
            } else {
                derives.push(input.parse()?);
            }
            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }

        Ok(Self { derives, version })
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
    let Args { derives, version } = syn::parse2(args)?;
    let implementation = implementation(&input, direction, version)?;
    let input = crate::contract::expand_with_derives(input, derives.iter());

    Ok(quote! {
        #input
        #implementation
    })
}

fn implementation(
    input: &DeriveInput,
    direction: Direction,
    version: Option<LitInt>,
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
        Direction::Agent => quote! {
            impl #impl_generics ::vmux_api::AgentRequestContract for #ident #type_generics #where_clause {}
        },
    };

    Ok(quote! {
        impl #impl_generics ::vmux_api::BinEvent for #ident #type_generics #where_clause {
            const ID: &'static str = #id;
            const NAME: &'static str = #name;
            const PERMISSION: &'static str = stringify!(#ident);
            #version
        }

        #direction_impl
    })
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
