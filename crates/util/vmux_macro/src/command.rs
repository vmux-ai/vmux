use heck::ToSnakeCase;
use proc_macro2::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Data, DeriveInput, Fields, Ident, LitStr, Path, Token, parse_quote};

struct Args {
    id: Option<LitStr>,
    owner: Path,
    message: bool,
    binding: bool,
}

impl Parse for Args {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut id = None;
        let mut owner = None;
        let mut message = false;
        let mut binding = false;
        while !input.is_empty() {
            let key: Ident = input.parse()?;
            if key == "message" && !input.peek(Token![=]) {
                message = true;
            } else if key == "binding" && !input.peek(Token![=]) {
                binding = true;
            } else {
                input.parse::<Token![=]>()?;
                match key.to_string().as_str() {
                    "id" => id = Some(input.parse()?),
                    "owner" => owner = Some(input.parse()?),
                    _ => return Err(syn::Error::new_spanned(key, "unknown command option")),
                }
            }
            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }
        Ok(Self {
            id,
            owner: owner.unwrap_or_else(|| parse_quote!(crate::Feature)),
            message,
            binding,
        })
    }
}

pub(crate) fn expand(args: TokenStream, mut input: DeriveInput) -> syn::Result<TokenStream> {
    let args = syn::parse2::<Args>(args)?;
    let ident = &input.ident;
    let owner = args.owner;
    if args.message && args.binding {
        return Err(syn::Error::new_spanned(
            ident,
            "command cannot be both message and binding",
        ));
    }
    if args.message {
        if !input.generics.params.is_empty() {
            return Err(syn::Error::new_spanned(
                &input.generics,
                "command messages cannot be generic",
            ));
        }
        return Ok(quote! {
            #input

            ::vmux_command::__private::inventory::submit! {
                ::vmux_command::__private::CommandMessageRegistration::of::<#ident, #owner>()
            }
        });
    }
    if args.binding {
        if !input.generics.params.is_empty() {
            return Err(syn::Error::new_spanned(
                &input.generics,
                "command bindings cannot be generic",
            ));
        }
        return Ok(quote! {
            #input

            ::vmux_command::__private::inventory::submit! {
                ::vmux_command::CommandBindingRegistration::of::<#ident, #owner>(::core::option::Option::None)
            }

            ::vmux_feature::__private::inventory::submit! {
                ::vmux_feature::CommandBindingRegistration::of::<#owner>(::core::option::Option::None)
            }
        });
    }
    if !matches!(&input.data, Data::Struct(data) if matches!(data.fields, Fields::Unit)) {
        return Err(syn::Error::new_spanned(
            &input,
            "command bindings must be unit structs",
        ));
    }
    let id = args.id.unwrap_or_else(|| {
        let mut name = ident.to_string();
        for suffix in ["Request", "Binding"] {
            if let Some(value) = name.strip_suffix(suffix) {
                name = value.to_string();
                break;
            }
        }
        LitStr::new(&name.to_snake_case(), ident.span())
    });
    input
        .attrs
        .push(parse_quote!(#[derive(::bevy::prelude::Component)]));
    let (impl_generics, type_generics, where_clause) = input.generics.split_for_impl();
    Ok(quote! {
        #input

        impl #impl_generics ::vmux_command::CommandBinding for #ident #type_generics #where_clause {
            fn for_command(id: &str) -> ::core::option::Option<Self> {
                (id == #id).then_some(Self)
            }
        }

        ::vmux_command::__private::inventory::submit! {
            ::vmux_command::CommandBindingRegistration::of::<#ident #type_generics, #owner>(::core::option::Option::Some(#id))
        }

        ::vmux_feature::__private::inventory::submit! {
            ::vmux_feature::CommandBindingRegistration::of::<#owner>(::core::option::Option::Some(#id))
        }
    })
}
