use proc_macro2::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Data, DeriveInput, Fields, Ident, Path};

struct Args {
    path: Path,
}

impl Parse for Args {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let path = input.parse()?;
        if !input.is_empty() {
            return Err(input.error("expected one service message variant"));
        }
        Ok(Self { path })
    }
}

pub(crate) fn expand(args: TokenStream, input: DeriveInput) -> syn::Result<TokenStream> {
    let Args { path } = syn::parse2(args)?;
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
    let segments = path.segments.iter().collect::<Vec<_>>();
    let pattern = if let [variant] = segments.as_slice() {
        let variant = &variant.ident;
        quote! {
            ::vmux_api::protocol::ServiceMessage::#variant { #(#names,)* .. }
        }
    } else if let [event, variant] = segments.as_slice() {
        let event_name = event.ident.to_string();
        let Some(outer_name) = event_name.strip_suffix("Event") else {
            return Err(syn::Error::new_spanned(
                &path,
                "nested service message enums must end in `Event`",
            ));
        };
        let outer = Ident::new(outer_name, event.ident.span());
        let event = &event.ident;
        let variant = &variant.ident;
        quote! {
            ::vmux_api::protocol::ServiceMessage::#outer(
                ::vmux_api::protocol::#event::#variant { #(#names,)* .. }
            )
        }
    } else {
        return Err(syn::Error::new_spanned(
            &path,
            "expected `Variant` or `Event::Variant`",
        ));
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
        let output = expand(quote!(SharedEvent::AgentDelta), input)
            .unwrap()
            .to_string();

        assert!(output.contains("ServiceMessage :: Shared"));
        assert!(output.contains("SharedEvent :: AgentDelta"));
    }
}
