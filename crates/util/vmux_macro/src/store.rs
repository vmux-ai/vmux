use proc_macro2::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream, Parser};
use syn::punctuated::Punctuated;
use syn::{Data, DeriveInput, Fields, Ident, Token, Type};

pub(crate) struct Declaration {
    item: DeriveInput,
    args: Arguments,
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
        let args = syn::parse2::<Arguments>(args)?;
        if args.types.is_empty() {
            return Err(syn::Error::new_spanned(
                &item,
                "a store schema must declare at least one type",
            ));
        }
        Ok(Self { item, args })
    }

    pub(crate) fn parse_types(args: TokenStream, item: DeriveInput) -> syn::Result<Self> {
        Punctuated::<Type, Token![,]>::parse_terminated.parse2(args.clone())?;
        Self::parse(args, item)
    }

    pub(crate) fn expand_types(&self) -> TokenStream {
        let item = &self.item;
        let name = &item.ident;
        let types = &self.args.types;
        quote! {
            #item

            impl ::vmux_ecs::types::Schema for #name {
                fn register(registry: &mut ::vmux_ecs::types::TypeRegistry) {
                    #(registry.register::<#types>();)*
                }
            }
        }
    }

    pub(crate) fn expand(&self) -> TokenStream {
        let item = &self.item;
        let name = &item.ident;
        let types = &self.args.types;
        let domain = match &self.args.domain {
            Some(domain) => quote!(#domain),
            None => quote!(Self),
        };
        let tracking = if self.args.track {
            quote! {
                fn track(app: &mut ::vmux_ecs::store::App) {
                    use ::vmux_ecs::store::StoreAppExt;
                    #(app.track_store::<#types>();)*
                }
            }
        } else {
            TokenStream::new()
        };
        let clause = &item.generics.where_clause;
        quote! {
            #item

            impl ::vmux_ecs::store::Schema for #name #clause {
                fn register(registry: &mut ::vmux_ecs::store::TypeRegistry) {
                    #(
                        registry.register::<#types>();
                        registry.register_type_data::<
                            #types,
                            ::vmux_ecs::store::StoreDomain<#domain>,
                        >();
                    )*
                }

                fn filter(registry: &::vmux_ecs::store::TypeRegistry) -> ::vmux_ecs::store::WorldFilter {
                    ::vmux_ecs::store::StoreDomain::<#domain>::filter(registry)
                }

                #tracking
            }
        }
    }
}

struct Arguments {
    domain: Option<Type>,
    track: bool,
    types: Vec<Type>,
}

impl Parse for Arguments {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut domain = None;
        let mut track = false;
        if input.peek(Ident) && input.peek2(Token![=]) {
            let option: Ident = input.parse()?;
            if option != "domain" {
                return Err(syn::Error::new_spanned(option, "expected domain"));
            }
            input.parse::<Token![=]>()?;
            domain = Some(input.parse()?);
            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
                let option: Ident = input.parse()?;
                if option != "track" {
                    return Err(syn::Error::new_spanned(option, "expected track"));
                }
                track = true;
            }
            input.parse::<Token![;]>()?;
        }
        let declared = Punctuated::<Type, Token![,]>::parse_terminated(input)?;
        let mut types = Vec::new();
        for ty in declared {
            types.push(ty);
        }
        Ok(Self {
            domain,
            track,
            types,
        })
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
                quote!(domain = WorkspaceStore, changed; Name),
                parse_quote!(
                    struct Store;
                ),
            ),
            (
                quote!(scope = WorkspaceStore; Name),
                parse_quote!(
                    struct Store;
                ),
            ),
            (
                quote!(domain = WorkspaceStore, track;),
                parse_quote!(
                    struct Store;
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

    #[test]
    fn reflection_declarations_reject_store_options_and_invalid_schema_shapes() {
        let cases = [
            (
                quote!(domain = WorkspaceStore; Name),
                parse_quote!(
                    struct Types;
                ),
            ),
            (
                quote!(Name),
                parse_quote!(
                    struct Types(String);
                ),
            ),
            (
                quote!(Name),
                parse_quote!(
                    struct Types<T>;
                ),
            ),
            (
                quote!(),
                parse_quote!(
                    struct Types;
                ),
            ),
        ];
        for (args, item) in cases {
            assert!(Declaration::parse_types(args, item).is_err());
        }
        assert!(
            Declaration::parse_types(
                quote!(Name, Cwd),
                parse_quote!(
                    struct Types;
                )
            )
            .is_ok()
        );
    }
}
