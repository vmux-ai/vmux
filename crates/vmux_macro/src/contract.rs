use proc_macro2::TokenStream;
use quote::quote;
use syn::parse::Parser;
use syn::punctuated::Punctuated;
use syn::{DeriveInput, Path, Token};

pub(crate) fn expand(args: TokenStream, input: DeriveInput) -> syn::Result<TokenStream> {
    let derives = Punctuated::<Path, Token![,]>::parse_terminated.parse2(args)?;
    Ok(expand_with_derives(input, derives.iter()))
}

pub(crate) fn expand_with_derives<'a>(
    input: DeriveInput,
    derives: impl IntoIterator<Item = &'a Path>,
) -> TokenStream {
    let derives = derives
        .into_iter()
        .filter(|derive| {
            let Some(name) = derive.segments.last() else {
                return true;
            };
            !matches!(
                name.ident.to_string().as_str(),
                "Debug" | "Clone" | "PartialEq" | "Serialize" | "Deserialize" | "Archive"
            )
        })
        .collect::<Vec<_>>();
    quote! {
        #[derive(
            Debug,
            Clone,
            PartialEq,
            ::serde::Serialize,
            ::serde::Deserialize,
            ::rkyv::Archive,
            ::rkyv::Serialize,
            ::rkyv::Deserialize,
            #(#derives),*
        )]
        #input
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::{Item, parse_quote, parse2};

    #[test]
    fn contract_adds_serialization_derives_and_requested_extras() {
        let output = expand(quote! { Copy, Eq }, parse_quote! { pub struct Event; }).unwrap();
        let file = parse2::<syn::File>(output).unwrap();
        let Item::Struct(item) = &file.items[0] else {
            panic!("expected struct");
        };
        let mut derives = Vec::new();
        item.attrs[0]
            .parse_nested_meta(|meta| {
                derives.push(
                    meta.path
                        .segments
                        .iter()
                        .map(|segment| segment.ident.to_string())
                        .collect::<Vec<_>>()
                        .join("::"),
                );
                Ok(())
            })
            .unwrap();
        assert_eq!(
            derives,
            [
                "Debug",
                "Clone",
                "PartialEq",
                "serde::Serialize",
                "serde::Deserialize",
                "rkyv::Archive",
                "rkyv::Serialize",
                "rkyv::Deserialize",
                "Copy",
                "Eq",
            ]
        );
    }
}
