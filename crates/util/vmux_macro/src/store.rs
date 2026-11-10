use proc_macro2::TokenStream;
use quote::quote;
use syn::parse::Parser;
use syn::punctuated::Punctuated;
use syn::{Data, DeriveInput, Fields, Token, Type};

pub(crate) struct Declaration {
    item: DeriveInput,
    types: Vec<Type>,
}

impl Declaration {
    pub(crate) fn parse(args: TokenStream, item: DeriveInput) -> syn::Result<Self> {
        if !matches!(&item.data, Data::Struct(data) if matches!(&data.fields, Fields::Unit)) {
            return Err(syn::Error::new_spanned(
                &item,
                "a store schema must be a unit struct",
            ));
        }
        if !item.generics.params.is_empty() {
            return Err(syn::Error::new_spanned(
                &item.generics,
                "a store schema cannot have generic parameters",
            ));
        }
        let declared = Punctuated::<Type, Token![,]>::parse_terminated.parse2(args)?;
        if declared.is_empty() {
            return Err(syn::Error::new_spanned(
                &item,
                "a store schema must declare at least one type",
            ));
        }
        let mut types = Vec::new();
        for ty in declared {
            types.push(ty);
        }
        Ok(Self { item, types })
    }

    pub(crate) fn expand(&self) -> TokenStream {
        let item = &self.item;
        let name = &item.ident;
        let types = &self.types;
        let clause = &item.generics.where_clause;
        quote! {
            #item

            impl ::vmux_ecs::store::Schema for #name #clause {
                fn register(registry: &mut ::vmux_ecs::store::TypeRegistry) {
                    #(
                        registry.register::<#types>();
                        registry.register_type_data::<
                            #types,
                            ::vmux_ecs::store::StoreDomain<Self>,
                        >();
                    )*
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_quote;

    #[test]
    fn store_schemas_reject_payloads_generics_and_empty_declarations() {
        let cases = [
            (
                quote!(Name),
                parse_quote!(
                    struct Store(String);
                ),
            ),
            (
                quote!(Name),
                parse_quote!(
                    struct Store<T>;
                ),
            ),
            (
                quote!(),
                parse_quote!(
                    struct Store;
                ),
            ),
        ];
        for (args, item) in cases {
            assert!(Declaration::parse(args, item).is_err());
        }
    }
}
