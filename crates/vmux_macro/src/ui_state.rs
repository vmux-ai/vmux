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
    let (Some(_), Some(patches)) = (sequence, patches) else {
        if sequence.is_some() || patches.is_some() {
            return Err(syn::Error::new_spanned(
                fields,
                "batched UiState requires both sequence and patches fields",
            ));
        }
        return Ok(quote! {
            impl #impl_generics ::vmux_api::UiState for #ident #type_generics #where_clause {
                type Update = Self;

                fn from_updates(
                    _sequence: u64,
                    mut updates: ::std::vec::Vec<Self::Update>,
                ) -> Self {
                    updates.pop().expect("UI state update is never empty")
                }

                fn retained(&self) -> ::core::option::Option<Self> {
                    ::core::option::Option::Some(self.clone())
                }
            }
        });
    };
    let patch = vec_element(&patches.ty).ok_or_else(|| {
        syn::Error::new_spanned(&patches.ty, "UiState patches must have type Vec<Patch>")
    })?;

    Ok(quote! {
        impl #impl_generics ::vmux_api::UiState for #ident #type_generics #where_clause {
            type Update = #patch;

            fn from_updates(sequence: u64, updates: ::std::vec::Vec<Self::Update>) -> Self {
                Self { sequence, patches: updates }
            }

            fn retained(&self) -> ::core::option::Option<Self> {
                ::core::option::Option::None
            }
        }

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
    match &input.data {
        Data::Enum(data) => derive_enum_patch(input, data),
        Data::Struct(data) => derive_struct_patch(input, data),
        _ => Err(syn::Error::new_spanned(
            &input.ident,
            "UiStatePatch requires an enum or struct",
        )),
    }
}

fn derive_enum_patch(input: &DeriveInput, data: &syn::DataEnum) -> syn::Result<TokenStream> {
    let ident = &input.ident;
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
        let stored = &fields.unnamed[0].ty;
        let payload = boxed_inner(stored).unwrap_or(stored);
        let variant = &variant.ident;
        let constructor = if boxed_inner(stored).is_some() {
            quote! { ::std::boxed::Box::new(payload) }
        } else {
            quote! { payload }
        };
        let payload_ref = if boxed_inner(stored).is_some() {
            quote! { payload.as_ref() }
        } else {
            quote! { payload }
        };
        implementations.push(quote! {
            impl #impl_generics ::core::convert::From<#payload>
                for #ident #type_generics #where_clause
            {
                fn from(payload: #payload) -> Self {
                    Self::#variant(#constructor)
                }
            }

            impl #impl_generics ::vmux_api::UiStatePatch<#payload>
                for #ident #type_generics #where_clause
            {
                fn payload(&self) -> ::core::option::Option<&#payload> {
                    let Self::#variant(payload) = self else {
                        return ::core::option::Option::None;
                    };
                    ::core::option::Option::Some(#payload_ref)
                }
            }
        });
    }

    Ok(quote! { #(#implementations)* })
}

fn derive_struct_patch(input: &DeriveInput, data: &syn::DataStruct) -> syn::Result<TokenStream> {
    let ident = &input.ident;
    let Fields::Named(fields) = &data.fields else {
        return Err(syn::Error::new_spanned(
            &data.fields,
            "UiStatePatch structs require named optional fields",
        ));
    };
    let generics = &input.generics;
    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();
    let mut implementations = Vec::new();
    for field in &fields.named {
        let field_name = field.ident.as_ref().unwrap();
        let stored = option_inner(&field.ty).ok_or_else(|| {
            syn::Error::new_spanned(&field.ty, "UiStatePatch fields require type Option<T>")
        })?;
        let payload = boxed_inner(stored).unwrap_or(stored);
        let value = if boxed_inner(stored).is_some() {
            quote! { ::std::boxed::Box::new(payload) }
        } else {
            quote! { payload }
        };
        let payload_ref = if boxed_inner(stored).is_some() {
            quote! { self.#field_name.as_deref() }
        } else {
            quote! { self.#field_name.as_ref() }
        };
        let empty_fields = fields.named.iter().filter_map(|candidate| {
            let candidate = candidate.ident.as_ref()?;
            (candidate != field_name).then(|| quote! { #candidate: ::core::option::Option::None })
        });
        implementations.push(quote! {
            impl #impl_generics ::core::convert::From<#payload>
                for #ident #type_generics #where_clause
            {
                fn from(payload: #payload) -> Self {
                    Self {
                        #field_name: ::core::option::Option::Some(#value),
                        #(#empty_fields),*
                    }
                }
            }

            impl #impl_generics ::vmux_api::UiStatePatch<#payload>
                for #ident #type_generics #where_clause
            {
                fn payload(&self) -> ::core::option::Option<&#payload> {
                    #payload_ref
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

fn boxed_inner(ty: &Type) -> Option<&Type> {
    let Type::Path(path) = ty else {
        return None;
    };
    let segment = path.path.segments.last()?;
    if segment.ident != "Box" {
        return None;
    }
    let PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return None;
    };
    let Some(GenericArgument::Type(inner)) = arguments.args.first() else {
        return None;
    };
    Some(inner)
}

fn option_inner(ty: &Type) -> Option<&Type> {
    let Type::Path(path) = ty else {
        return None;
    };
    let segment = path.path.segments.last()?;
    if segment.ident != "Option" {
        return None;
    }
    let PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return None;
    };
    let Some(GenericArgument::Type(inner)) = arguments.args.first() else {
        return None;
    };
    Some(inner)
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
    fn snapshot_state_implements_update_reduction() {
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

    #[test]
    fn boxed_patch_maps_the_unboxed_payload_type() {
        let input = parse_quote! {
            pub enum EditorPatch {
                Snapshot(Box<Snapshot>),
            }
        };
        let output = derive_patch(&input).unwrap();
        let file = parse2::<syn::File>(output).unwrap();
        let rendered = quote!(#file).to_string();

        assert!(rendered.contains("From < Snapshot >"));
        assert!(rendered.contains("UiStatePatch < Snapshot >"));
        assert!(rendered.contains("Box :: new (payload)"));
        assert!(rendered.contains("payload . as_ref ()"));
    }

    #[test]
    fn struct_patch_maps_each_optional_field() {
        let input = parse_quote! {
            pub struct EditorPatch {
                pub meta: Option<MetaEvent>,
                pub snapshot: Option<Box<Snapshot>>,
            }
        };
        let output = derive_patch(&input).unwrap();
        let file = parse2::<syn::File>(output).unwrap();

        assert_eq!(file.items.len(), 4);
        assert!(file.items.iter().all(|item| matches!(item, Item::Impl(_))));
    }
}
