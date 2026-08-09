use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput};

/// Expand `#[derive(VariantNames)]` into the enum's `VARIANT_NAMES` constant.
pub fn expand(input: DeriveInput) -> syn::Result<TokenStream> {
    let name = &input.ident;
    let Data::Enum(data) = &input.data else {
        return Err(syn::Error::new_spanned(
            name,
            "#[derive(VariantNames)] applies to an enum",
        ));
    };

    let mut variants = Vec::with_capacity(data.variants.len());
    for variant in &data.variants {
        variants.push(variant.ident.to_string());
    }

    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    Ok(quote! {
        impl #impl_generics #name #ty_generics #where_clause {
            /// Every variant of this enum, named, in declaration order.
            pub const VARIANT_NAMES: &'static [&'static str] = &[#(#variants),*];
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variant_names_lists_every_variant_in_declaration_order() {
        let input: DeriveInput = syn::parse_quote! {
            enum Surface {
                Unit,
                Tuple(u8),
                Named { field: u8 },
            }
        };

        let tokens = expand(input).expect("enum should expand").to_string();

        assert!(
            tokens.contains(r#"& ["Unit" , "Tuple" , "Named"]"#),
            "{tokens}"
        );
    }

    #[test]
    fn variant_names_rejects_a_struct() {
        let input: DeriveInput = syn::parse_quote! {
            struct NotAnEnum {
                field: u8,
            }
        };

        assert!(expand(input).is_err());
    }
}
