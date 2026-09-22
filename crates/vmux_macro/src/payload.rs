use proc_macro2::TokenStream;
use quote::quote;
use syn::parse::Parser;
use syn::punctuated::Punctuated;
use syn::{DeriveInput, Path, Token};

pub(crate) fn expand(args: TokenStream, input: DeriveInput) -> syn::Result<TokenStream> {
    let derives = Punctuated::<Path, Token![,]>::parse_terminated.parse2(args)?;
    let derives = derives.iter();
    Ok(quote! {
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
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::{Item, parse_quote, parse2};

    #[test]
    fn payload_adds_wire_derives_and_requested_extras() {
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
