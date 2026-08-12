use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, Type};

/// Expand `#[string_id]` on a newtype over `String`.
///
/// An attribute rather than a derive so the thirteen derives every id needs are written once here
/// instead of on each type, and rather than a `macro_rules!` that generates the struct too: this
/// way `RoomId` is a real declaration that go-to-definition finds and a doc comment can describe.
pub fn expand(input: DeriveInput) -> syn::Result<TokenStream> {
    let name = &input.ident;
    let docs = input
        .attrs
        .iter()
        .filter(|attr| attr.path().is_ident("doc"));
    reject_unless_string_newtype(&input)?;

    Ok(quote! {
        #(#docs)*
        #[derive(
            Clone,
            Debug,
            serde::Deserialize,
            Eq,
            Hash,
            Ord,
            PartialEq,
            PartialOrd,
            serde::Serialize,
            rkyv::Archive,
            rkyv::Serialize,
            rkyv::Deserialize,
        )]
        #[serde(transparent)]
        pub struct #name(pub String);

        impl #name {
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl From<String> for #name {
            fn from(value: String) -> Self {
                Self(value)
            }
        }

        impl From<&str> for #name {
            fn from(value: &str) -> Self {
                Self(value.to_string())
            }
        }

        impl std::fmt::Display for #name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }
    })
}

/// The expansion replaces the body outright, so anything other than a `String` newtype would be
/// silently discarded rather than rejected.
fn reject_unless_string_newtype(input: &DeriveInput) -> syn::Result<()> {
    let Data::Struct(data) = &input.data else {
        return Err(syn::Error::new_spanned(
            input,
            "#[string_id] applies to a struct",
        ));
    };
    let Fields::Unnamed(fields) = &data.fields else {
        return Err(syn::Error::new_spanned(
            &data.fields,
            "#[string_id] applies to a newtype: struct Name(pub String)",
        ));
    };
    if fields.unnamed.len() != 1 {
        return Err(syn::Error::new_spanned(
            fields,
            "#[string_id] applies to a newtype with exactly one field",
        ));
    }
    let Type::Path(path) = &fields.unnamed[0].ty else {
        return Err(syn::Error::new_spanned(
            &fields.unnamed[0].ty,
            "#[string_id] wraps a String",
        ));
    };
    if !path.path.is_ident("String") {
        return Err(syn::Error::new_spanned(
            &fields.unnamed[0].ty,
            "#[string_id] wraps a String",
        ));
    }
    Ok(())
}
