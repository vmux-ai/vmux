use heck::ToSnakeCase;
use proc_macro2::TokenStream;
use quote::quote;
use serde::Deserialize;
use std::path::PathBuf;
use syn::parse::{Parse, ParseStream};
use syn::{Data, DeriveInput, Fields, Ident, LitStr, Token, parse_quote};

#[derive(Deserialize)]
struct FeatureManifest {
    #[serde(default)]
    commands: Vec<Command>,
}

#[derive(Deserialize)]
struct Command {
    id: String,
}

struct Args {
    id: Option<LitStr>,
    file: LitStr,
    message: bool,
    binding: bool,
}

impl Parse for Args {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut id = None;
        let mut file = None;
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
                    "file" => file = Some(input.parse()?),
                    _ => return Err(syn::Error::new_spanned(key, "unknown command option")),
                }
            }
            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }
        Ok(Self {
            id,
            file: file.unwrap_or_else(|| LitStr::new("src/feature.ron", input.span())),
            message,
            binding,
        })
    }
}

pub(crate) fn expand(args: TokenStream, mut input: DeriveInput) -> syn::Result<TokenStream> {
    let args = syn::parse2::<Args>(args)?;
    let ident = &input.ident;
    let file = args.file;
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
            const _: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/", #file));

            #input

            ::vmux_command::__private::inventory::submit! {
                ::vmux_command::__private::CommandMessageRegistration::of::<#ident>()
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
            const _: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/", #file));

            #input

            ::vmux_command::__private::inventory::submit! {
                ::vmux_command::CommandBindingRegistration::of::<#ident>()
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
    let crate_root = std::env::var_os("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .ok_or_else(|| syn::Error::new(id.span(), "CARGO_MANIFEST_DIR is not set"))?;
    let path = crate_root.join(file.value());
    let source = std::fs::read_to_string(&path).map_err(|error| {
        syn::Error::new(
            id.span(),
            format!(
                "failed to read feature manifest {}: {error}",
                path.display()
            ),
        )
    })?;
    let manifest: FeatureManifest = ron::from_str(&source).map_err(|error| {
        syn::Error::new(
            id.span(),
            format!(
                "failed to parse feature manifest {}: {error}",
                path.display()
            ),
        )
    })?;
    let matches = manifest
        .commands
        .iter()
        .filter(|command| command.id == id.value())
        .count();
    if matches != 1 {
        return Err(syn::Error::new(
            id.span(),
            format!(
                "feature manifest {} must define command {} exactly once",
                path.display(),
                id.value()
            ),
        ));
    }
    input
        .attrs
        .push(parse_quote!(#[derive(::bevy::prelude::Component)]));
    let (impl_generics, type_generics, where_clause) = input.generics.split_for_impl();
    Ok(quote! {
        const _: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/", #file));

        #input

        impl #impl_generics ::vmux_command::CommandBinding for #ident #type_generics #where_clause {
            fn for_command(id: &str) -> ::core::option::Option<Self> {
                (id == #id).then_some(Self)
            }
        }

        ::vmux_command::__private::inventory::submit! {
            ::vmux_command::CommandBindingRegistration::of::<#ident #type_generics>()
        }
    })
}
