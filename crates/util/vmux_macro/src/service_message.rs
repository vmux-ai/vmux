use proc_macro2::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Data, DeriveInput, Fields, Ident, Path, parenthesized};

struct Args {
    outer: Ident,
    nested: Option<Path>,
}

impl Parse for Args {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let outer = input.parse()?;
        let nested = if input.peek(syn::token::Paren) {
            let content;
            parenthesized!(content in input);
            Some(content.parse()?)
        } else {
            None
        };
        if !input.is_empty() {
            return Err(input.error("expected one service message variant"));
        }
        Ok(Self { outer, nested })
    }
}

pub(crate) fn expand(args: TokenStream, input: DeriveInput) -> syn::Result<TokenStream> {
    let Args { outer, nested } = syn::parse2(args)?;
    let ident = &input.ident;
    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => fields,
            _ => {
                return Err(syn::Error::new_spanned(
                    &input,
                    "service messages require named fields",
                ));
            }
        },
        _ => {
            return Err(syn::Error::new_spanned(
                &input,
                "service messages must be structs",
            ));
        }
    };
    let names = fields
        .named
        .iter()
        .map(|field| field.ident.as_ref().expect("named field"))
        .collect::<Vec<_>>();
    let pattern = if let Some(nested) = nested {
        quote! {
            ::vmux_api::protocol::ServiceMessage::#outer(
                ::vmux_api::protocol::#nested { #(#names,)* .. }
            )
        }
    } else {
        quote! {
            ::vmux_api::protocol::ServiceMessage::#outer { #(#names,)* .. }
        }
    };

    Ok(quote! {
        #[derive(::bevy::ecs::message::Message)]
        #input

        impl ::vmux_core::service::ServiceMessageVariant for #ident {
            fn from_service_message(
                message: &::vmux_api::protocol::ServiceMessage,
            ) -> ::core::option::Option<Self> {
                let #pattern = message else {
                    return ::core::option::Option::None;
                };
                ::core::option::Option::Some(Self {
                    #(#names: #names.clone(),)*
                })
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;
    use syn::parse_quote;

    #[test]
    fn expands_direct_variant() {
        let input: DeriveInput = parse_quote! {
            pub struct ProcessCreated {
                pub process_id: ProcessId,
                pub pid: u32,
            }
        };
        let output = expand(quote!(ProcessCreated), input).unwrap().to_string();

        assert!(output.contains("ServiceMessage :: ProcessCreated"));
        assert!(output.contains("process_id : process_id . clone"));
        assert!(output.contains("derive (:: bevy :: ecs :: message :: Message)"));
    }

    #[test]
    fn expands_nested_variant() {
        let input: DeriveInput = parse_quote! {
            pub struct AgentDelta {
                pub sid: String,
                pub text: String,
            }
        };
        let output = expand(quote!(Shared(SharedEvent::AgentDelta)), input)
            .unwrap()
            .to_string();

        assert!(output.contains("ServiceMessage :: Shared"));
        assert!(output.contains("SharedEvent :: AgentDelta"));
    }
}
