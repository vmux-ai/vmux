use heck::ToSnakeCase;
use proc_macro2::TokenStream;
use quote::quote;
use syn::{DeriveInput, LitStr};

pub(crate) fn expand(input: DeriveInput) -> syn::Result<TokenStream> {
    let ident = &input.ident;
    let mut name = ident.to_string();
    for suffix in ["Args", "Input", "Tool"] {
        if let Some(value) = name.strip_suffix(suffix) {
            name = value.to_string();
            break;
        }
    }
    let name = LitStr::new(&name.to_snake_case(), ident.span());
    let (impl_generics, type_generics, where_clause) = input.generics.split_for_impl();

    Ok(quote! {
        #input

        impl #impl_generics ::vmux_tool::ToolInput for #ident #type_generics #where_clause {
            const NAME: &'static str = #name;
        }

        ::vmux_tool::__private::inventory::submit! {
            ::vmux_tool::ToolInputRegistration::of::<#ident #type_generics>()
        }
    })
}
