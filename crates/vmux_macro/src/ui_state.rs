use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, GenericArgument, PathArguments, Type};

pub(crate) fn derive_state(input: &DeriveInput) -> syn::Result<TokenStream> {
    let ident = &input.ident;
    let Data::Struct(data) = &input.data else {
        return Err(syn::Error::new_spanned(ident, "UiState requires a struct"));
    };
    let Fields::Named(fields) = &data.fields else {
        return Err(syn::Error::new_spanned(
            &data.fields,
            "UiState requires named fields",
        ));
    };
    let sequence = fields
        .named
        .iter()
        .find(|field| field.ident.as_ref().is_some_and(|name| name == "sequence"));
    let patches = fields
        .named
        .iter()
        .find(|field| field.ident.as_ref().is_some_and(|name| name == "patches"));
    let generics = &input.generics;
    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();
    let state = quote! {
        impl #impl_generics ::vmux_api::UiState for #ident #type_generics #where_clause {}
    };
    let (Some(_), Some(patches)) = (sequence, patches) else {
        if sequence.is_some() || patches.is_some() {
            return Err(syn::Error::new_spanned(
                fields,
                "batched UiState requires both sequence and patches fields",
            ));
        }
        return Ok(state);
    };
    let patch = vec_element(&patches.ty).ok_or_else(|| {
        syn::Error::new_spanned(&patches.ty, "UiState patches must have type Vec<Patch>")
    })?;

    Ok(quote! {
        #state

        impl #impl_generics ::vmux_api::BatchedUiState for #ident #type_generics #where_clause {
            type Patch = #patch;

            fn sequence(&self) -> u64 {
                self.sequence
            }

            fn patches(&self) -> &[Self::Patch] {
                &self.patches
            }

            fn from_parts(sequence: u64, patches: ::std::vec::Vec<Self::Patch>) -> Self {
                Self { sequence, patches }
            }
        }
    })
}

pub(crate) fn derive_patch(input: &DeriveInput) -> syn::Result<TokenStream> {
    let ident = &input.ident;
    let Data::Enum(data) = &input.data else {
        return Err(syn::Error::new_spanned(
            ident,
            "UiStatePatch requires an enum",
        ));
    };
    let generics = &input.generics;
    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();
    let mut implementations = Vec::new();

    for variant in &data.variants {
        let Fields::Unnamed(fields) = &variant.fields else {
            return Err(syn::Error::new_spanned(
                &variant.fields,
                "UiStatePatch variants require one unnamed field",
            ));
        };
        if fields.unnamed.len() != 1 {
            return Err(syn::Error::new_spanned(
                fields,
                "UiStatePatch variants require one unnamed field",
            ));
        }
        let payload = &fields.unnamed[0].ty;
        let variant = &variant.ident;
        implementations.push(quote! {
            impl #impl_generics ::core::convert::From<#payload>
                for #ident #type_generics #where_clause
            {
                fn from(payload: #payload) -> Self {
                    Self::#variant(payload)
                }
            }

            impl #impl_generics ::vmux_api::UiStatePatch<#payload>
                for #ident #type_generics #where_clause
            {
                fn payload(&self) -> ::core::option::Option<&#payload> {
                    let Self::#variant(payload) = self else {
                        return ::core::option::Option::None;
                    };
                    ::core::option::Option::Some(payload)
                }
            }
        });
    }

    Ok(quote! { #(#implementations)* })
}

fn vec_element(ty: &Type) -> Option<&Type> {
    let Type::Path(path) = ty else {
        return None;
    };
    let segment = path.path.segments.last()?;
    if segment.ident != "Vec" {
        return None;
    }
    let PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return None;
    };
    let Some(GenericArgument::Type(element)) = arguments.args.first() else {
        return None;
    };
    Some(element)
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::{Item, parse_quote, parse2};

    #[test]
    fn batched_state_uses_sequence_and_patch_fields() {
        let input = parse_quote! {
            pub struct EditorState {
                pub sequence: u64,
                pub patches: Vec<EditorPatch>,
            }
        };
        let output = derive_state(&input).unwrap();
        let file = parse2::<syn::File>(output).unwrap();
        assert!(matches!(
            file.items.as_slice(),
            [Item::Impl(_), Item::Impl(_)]
        ));
    }

    #[test]
    fn snapshot_state_only_implements_the_marker() {
        let input = parse_quote! {
            pub struct ToolState {
                pub loaded: bool,
            }
        };
        let output = derive_state(&input).unwrap();
        let file = parse2::<syn::File>(output).unwrap();
        assert!(matches!(file.items.as_slice(), [Item::Impl(_)]));
    }

    #[test]
    fn patch_maps_each_payload_type() {
        let input = parse_quote! {
            pub enum EditorPatch {
                Meta(MetaEvent),
                Cursor(CursorEvent),
            }
        };
        let output = derive_patch(&input).unwrap();
        let file = parse2::<syn::File>(output).unwrap();
        assert_eq!(file.items.len(), 4);
        assert!(file.items.iter().all(|item| matches!(item, Item::Impl(_))));
    }
}
