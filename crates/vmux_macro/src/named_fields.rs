use proc_macro2::TokenStream;
use quote::quote;
use syn::{Fields, Ident};

/// Build the constructor expression for a variant with `Fields::Named` where
/// every field receives its `Default::default()` value. Used by OsSubMenu /
/// DefaultShortcuts / CommandBar when binding a menu-id or shortcut to a
/// variant without a payload.
pub fn build_default_named_constructor(
    enum_ident: &Ident,
    variant_ident: &Ident,
    fields: &Fields,
) -> TokenStream {
    let Fields::Named(named) = fields else {
        return quote!(#enum_ident::#variant_ident);
    };
    let field_idents: Vec<&Ident> = named
        .named
        .iter()
        .map(|f| f.ident.as_ref().expect("named field"))
        .collect();
    quote! {
        #enum_ident::#variant_ident {
            #( #field_idents: ::core::default::Default::default(), )*
        }
    }
}
