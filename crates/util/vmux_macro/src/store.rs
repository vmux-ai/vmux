use proc_macro2::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream, Parser};
use syn::punctuated::Punctuated;
use syn::{Data, DeriveInput, Fields, Ident, Path, Token, Type};

pub(crate) struct Declaration {
    item: DeriveInput,
    crate_path: Path,
    args: Arguments,
}

impl Declaration {
    pub(crate) fn parse(
        args: TokenStream,
        item: DeriveInput,
        default_crate: &str,
    ) -> syn::Result<Self> {
        let crate_path = syn::parse_str::<Path>(default_crate)?;
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
        Ok(Self {
            item,
            crate_path,
            args,
        })
    }

    pub(crate) fn parse_types(args: TokenStream, item: DeriveInput) -> syn::Result<Self> {
        Punctuated::<Type, Token![,]>::parse_terminated.parse2(args.clone())?;
        Self::parse(args, item, "::vmux_ecs::store")
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
        let crate_path = &self.crate_path;
        let types = &self.args.types;
        let domain = match &self.args.domain {
            Some(domain) => quote!(#domain),
            None => quote!(Self),
        };
        let filter = match &self.args.filter {
            Some(filter) => quote!(#filter),
            None => quote!(#crate_path::With<#crate_path::Save>),
        };
        let tracking = if self.args.track {
            quote! {
                fn track(app: &mut #crate_path::App) {
                    use #crate_path::StoreAppExt;
                    #(app.track_domain::<#domain, #filter, #types>();)*
                }
            }
        } else {
            TokenStream::new()
        };
        let clause = &item.generics.where_clause;
        quote! {
            #item

            impl #crate_path::Schema for #name #clause {
                fn register(registry: &mut #crate_path::TypeRegistry) {
                    #(
                        registry.register::<#types>();
                        registry.register_type_data::<
                            #types,
                            #crate_path::StoreDomain<#domain>,
                        >();
                    )*
                }

                fn filter(registry: &#crate_path::TypeRegistry) -> #crate_path::WorldFilter {
                    #crate_path::StoreDomain::<#domain>::filter(registry)
                }

                #tracking
            }
        }
    }
}

struct Arguments {
    domain: Option<Type>,
    filter: Option<Type>,
    track: bool,
    types: Vec<Type>,
}

impl Arguments {
    fn has_options(input: ParseStream) -> bool {
        let fork = input.fork();
        while !fork.is_empty() {
            if fork.peek(Token![;]) {
                return true;
            }
            if fork.parse::<proc_macro2::TokenTree>().is_err() {
                return false;
            }
        }
        false
    }
}

impl Parse for Arguments {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut domain = None;
        let mut filter = None;
        let mut track = false;
        if Self::has_options(input) {
            while !input.peek(Token![;]) {
                let option: Ident = input.parse()?;
                match option.to_string().as_str() {
                    "domain" => {
                        if domain.is_some() {
                            return Err(syn::Error::new_spanned(option, "duplicate domain"));
                        }
                        input.parse::<Token![=]>()?;
                        domain = Some(input.parse()?);
                    }
                    "filter" => {
                        if filter.is_some() {
                            return Err(syn::Error::new_spanned(option, "duplicate filter"));
                        }
                        input.parse::<Token![=]>()?;
                        filter = Some(input.parse()?);
                    }
                    "track" => {
                        if track {
                            return Err(syn::Error::new_spanned(option, "duplicate track"));
                        }
                        track = true;
                    }
                    _ => {
                        return Err(syn::Error::new_spanned(
                            option,
                            "expected domain, filter, or track",
                        ));
                    }
                }
                if input.peek(Token![,]) {
                    input.parse::<Token![,]>()?;
                } else {
                    break;
                }
            }
            input.parse::<Token![;]>()?;
            if filter.is_some() && !track {
                return Err(syn::Error::new(
                    input.span(),
                    "filter option requires track",
                ));
            }
        }
        let declared = Punctuated::<Type, Token![,]>::parse_terminated(input)?;
        let mut types = Vec::new();
        for ty in declared {
            types.push(ty);
        }
        Ok(Self {
            domain,
            filter,
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
                quote!(filter = BookmarkFilter; Name),
                parse_quote!(
                    struct Store;
                ),
            ),
            (
                quote!(track, track; Name),
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
            assert!(Declaration::parse(args, item, "::vmux_ecs::store").is_err());
        }
    }

    #[test]
    fn store_schemas_support_standalone_track_and_custom_filters() {
        let standalone = Declaration::parse(
            quote!(track; Name, Title),
            parse_quote!(
                struct NotesStore;
            ),
            "::vmux_ecs::store",
        )
        .unwrap();
        let expanded = standalone.expand().to_string();
        assert!(expanded.contains("track_domain :: < Self , :: vmux_ecs :: store :: With < :: vmux_ecs :: store :: Save > , Name >"));
        assert!(expanded.contains("track_domain :: < Self , :: vmux_ecs :: store :: With < :: vmux_ecs :: store :: Save > , Title >"));

        let direct = Declaration::parse(
            quote!(track; Name, Title),
            parse_quote!(
                struct DirectNotesStore;
            ),
            "::vmux_store",
        )
        .unwrap();
        let expanded = direct.expand().to_string();
        assert!(expanded.contains("impl :: vmux_store :: Schema for DirectNotesStore"));
        assert!(expanded.contains(
            "track_domain :: < Self , :: vmux_store :: With < :: vmux_store :: Save > , Name >"
        ));

        let filtered = Declaration::parse(
            quote!(filter = BookmarkFilter, track; Pin, Name),
            parse_quote!(
                struct BookmarkStore;
            ),
            "::vmux_ecs::store",
        )
        .unwrap();
        let expanded = filtered.expand().to_string();
        assert!(expanded.contains("track_domain :: < Self , BookmarkFilter , Pin >"));
        assert!(expanded.contains("track_domain :: < Self , BookmarkFilter , Name >"));
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
