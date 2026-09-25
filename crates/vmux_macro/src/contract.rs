use proc_macro2::TokenStream;
use quote::quote;
use syn::parse::Parser;
use syn::punctuated::Punctuated;
use syn::{Data, DeriveInput, Path, Token, parse_quote};

pub(crate) fn expand(args: TokenStream, mut input: DeriveInput) -> syn::Result<TokenStream> {
    let args = Punctuated::<Path, Token![,]>::parse_terminated.parse2(args)?;
    let recursive = args.iter().any(|arg| arg.is_ident("recursive"));
    let derives = args.iter().filter(|arg| !arg.is_ident("recursive"));
    if recursive {
        add_recursive_rkyv_attributes(&mut input);
    }
    Ok(expand_with_derives(input, derives))
}

fn add_recursive_rkyv_attributes(input: &mut DeriveInput) {
    input.attrs.push(parse_quote!(
        #[rkyv(serialize_bounds(
            __S: rkyv::ser::Writer + rkyv::ser::Allocator,
            __S::Error: rkyv::rancor::Source
        ))]
    ));
    input.attrs.push(parse_quote!(
        #[rkyv(deserialize_bounds(__D::Error: rkyv::rancor::Source))]
    ));
    input.attrs.push(parse_quote!(
        #[rkyv(bytecheck(bounds(
            __C: rkyv::validation::ArchiveContext,
            __C::Error: rkyv::rancor::Source
        )))]
    ));
    match &mut input.data {
        Data::Struct(item) => add_omit_bounds(&mut item.fields),
        Data::Enum(item) => {
            for variant in &mut item.variants {
                add_omit_bounds(&mut variant.fields);
            }
        }
        Data::Union(item) => {
            for field in &mut item.fields.named {
                field.attrs.push(parse_quote!(#[rkyv(omit_bounds)]));
            }
        }
    }
}

fn add_omit_bounds(fields: &mut syn::Fields) {
    for field in fields {
        field.attrs.push(parse_quote!(#[rkyv(omit_bounds)]));
    }
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

    #[test]
    fn recursive_contract_owns_rkyv_bounds() {
        let output = expand(
            quote! { recursive, Eq },
            parse_quote! {
                pub enum Node {
                    Leaf(String),
                    Branch(Vec<Node>),
                }
            },
        )
        .unwrap();
        let file = parse2::<syn::File>(output).unwrap();
        let Item::Enum(item) = &file.items[0] else {
            panic!("expected enum");
        };
        assert_eq!(
            item.attrs
                .iter()
                .filter(|attribute| attribute.path().is_ident("rkyv"))
                .count(),
            3
        );
        assert!(item.variants.iter().all(|variant| {
            variant.fields.iter().all(|field| {
                field
                    .attrs
                    .iter()
                    .any(|attribute| attribute.path().is_ident("rkyv"))
            })
        }));
    }
}
