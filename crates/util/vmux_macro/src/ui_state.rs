use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, GenericArgument, PathArguments, Type};

pub(crate) fn derive_state(input: &DeriveInput, patch: Option<&Type>) -> syn::Result<TokenStream> {
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
    if fields.named.iter().any(|field| {
        field
            .ident
            .as_ref()
            .is_some_and(|name| name == "sequence" || name == "patches")
    }) {
        return Err(syn::Error::new_spanned(
            fields,
            "UiState is a retained snapshot and cannot contain sequence or patches fields",
        ));
    }
    let generics = &input.generics;
    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();
    if let Some(patch) = patch {
        return Ok(quote! {
            impl #impl_generics ::vmux_api::UiState for #ident #type_generics #where_clause {
                type Update = #patch;

                fn from_updates(
                    previous: ::core::option::Option<Self>,
                    updates: ::std::vec::Vec<Self::Update>,
                ) -> Self {
                    let mut state = previous.unwrap_or_default();
                    for update in updates {
                        <Self as ::vmux_api::UiStateProjection<#patch>>::apply(&mut state, update);
                    }
                    state
                }

                fn retained(&self) -> ::core::option::Option<Self> {
                    ::core::option::Option::Some(self.clone())
                }
            }
        });
    }
    Ok(quote! {
        impl #impl_generics ::vmux_api::UiState for #ident #type_generics #where_clause {
            type Update = Self;

            fn from_updates(
                _previous: ::core::option::Option<Self>,
                mut updates: ::std::vec::Vec<Self::Update>,
            ) -> Self {
                updates.pop().expect("UI state update is never empty")
            }

            fn retained(&self) -> ::core::option::Option<Self> {
                ::core::option::Option::Some(self.clone())
            }
        }
    })
}

pub(crate) fn derive_patch(input: &DeriveInput) -> syn::Result<TokenStream> {
    let Data::Struct(data) = &input.data else {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "UiStatePatch requires a named-field struct",
        ));
    };
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
    fn retained_state_rejects_legacy_batch_fields() {
        let input = parse_quote! {
            pub struct EditorState {
                pub sequence: u64,
                pub patches: Vec<EditorPatch>,
            }
        };
        let error = derive_state(&input, None).unwrap_err();
        assert!(error.to_string().contains("retained snapshot"));
    }

    #[test]
    fn snapshot_state_implements_update_reduction() {
        let input = parse_quote! {
            pub struct ToolState {
                pub loaded: bool,
            }
        };
        let output = derive_state(&input, None).unwrap();
        let file = parse2::<syn::File>(output).unwrap();
        assert!(matches!(file.items.as_slice(), [Item::Impl(_)]));
    }

    #[test]
    fn patch_maps_each_payload_type() {
        let input = parse_quote! {
            pub struct EditorPatch {
                pub meta: Option<MetaEvent>,
                pub cursor: Option<CursorEvent>,
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
            pub struct EditorPatch {
                pub snapshot: Option<Box<Snapshot>>,
            }
        };
        let output = derive_patch(&input).unwrap();
        let file = parse2::<syn::File>(output).unwrap();
        let rendered = quote!(#file).to_string();

        assert!(rendered.contains("From < Snapshot >"));
        assert!(rendered.contains("UiStatePatch < Snapshot >"));
        assert!(rendered.contains("Box :: new (payload)"));
        assert!(rendered.contains("self . snapshot . as_deref ()"));
    }
}
