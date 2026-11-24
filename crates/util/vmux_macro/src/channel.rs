use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{
    Attribute, Data, DeriveInput, Field, Fields, GenericArgument, Ident, Path, PathArguments,
    Token, Type,
};

pub(crate) struct Declaration {
    item: DeriveInput,
    crate_path: Path,
    fields: Vec<ChannelField>,
}

impl Declaration {
    pub(crate) fn parse(
        args: TokenStream,
        mut item: DeriveInput,
        default_crate: &str,
    ) -> syn::Result<Self> {
        let crate_path = if args.is_empty() {
            syn::parse_str::<Path>(default_crate)?
        } else {
            syn::parse2::<Arguments>(args)?.crate_path
        };
        if !item.generics.params.is_empty() {
            return Err(syn::Error::new_spanned(
                &item.generics,
                "channels struct cannot have generic parameters",
            ));
        }
        let Data::Struct(data) = &mut item.data else {
            return Err(syn::Error::new_spanned(
                &item,
                "channels attribute requires a struct with named fields",
            ));
        };
        let Fields::Named(named) = &mut data.fields else {
            return Err(syn::Error::new_spanned(
                &item,
                "channels attribute requires named fields",
            ));
        };
        let mut fields = Vec::new();
        let mut has_channel = false;
        for field in &mut named.named {
            let parsed = ChannelField::parse(field)?;
            if !matches!(parsed.kind, FieldKind::Default) {
                has_channel = true;
            }
            fields.push(parsed);
        }
        if !has_channel {
            return Err(syn::Error::new_spanned(
                &item,
                "channels struct must declare at least one channel sender field",
            ));
        }
        Ok(Self {
            item,
            crate_path,
            fields,
        })
    }

    pub(crate) fn expand(&self) -> TokenStream {
        let item = &self.item;
        let name = &item.ident;
        let vis = &item.vis;
        let crate_path = &self.crate_path;

        let mut open_stmts = Vec::new();
        let mut open_waker_stmts = Vec::new();
        let mut sender_inits = Vec::new();
        let mut receiver_idents = Vec::new();
        let mut plugin_exprs = Vec::new();

        for (index, field) in self.fields.iter().enumerate() {
            let ident = &field.ident;
            match &field.kind {
                FieldKind::Default => {
                    sender_inits.push(quote!(#ident: ::core::default::Default::default()));
                }
                FieldKind::Spawn { payload, manual } => {
                    let rx = format_ident!("__vmux_channel_rx_{index}");
                    open_stmts.push(quote! {
                        let (#ident, #rx) = #crate_path::SpawnReceiver::<#payload>::unbounded();
                    });
                    open_waker_stmts.push(quote! {
                        let (#ident, #rx) = #crate_path::SpawnReceiver::<#payload>::unbounded_with_waker(
                            ::core::clone::Clone::clone(&waker),
                        );
                    });
                    sender_inits.push(quote!(#ident));
                    receiver_idents.push(rx);
                    if !manual {
                        plugin_exprs.push(quote! {
                            #crate_path::AsyncChannelPlugin::spawn::<#payload>().components()
                        });
                    }
                }
                FieldKind::Entity { payload, manual } => {
                    let rx = format_ident!("__vmux_channel_rx_{index}");
                    open_stmts.push(quote! {
                        let (#ident, #rx) = #crate_path::AsyncChannel::<#payload>::unbounded();
                    });
                    open_waker_stmts.push(quote! {
                        let (#ident, #rx) = #crate_path::AsyncChannel::<#payload>::unbounded_with_waker(
                            ::core::clone::Clone::clone(&waker),
                        );
                    });
                    sender_inits.push(quote!(#ident));
                    receiver_idents.push(rx);
                    if !manual {
                        plugin_exprs.push(quote! {
                            #crate_path::AsyncChannelPlugin::<#payload>::default()
                        });
                    }
                }
            }
        }

        let receiver_bundle = chunk_tuple(
            receiver_idents
                .iter()
                .map(|ident| quote!(#ident))
                .collect::<Vec<_>>(),
        );
        let add_plugins = if plugin_exprs.is_empty() {
            quote! {
                let _ = app;
            }
        } else {
            let chunks = plugin_exprs
                .chunks(8)
                .map(|chunk| quote!(app.add_plugins((#(#chunk,)*));));
            quote!(#(#chunks)*)
        };

        quote! {
            #item

            impl #name {
                #[allow(dead_code)]
                #vis fn open() -> (Self, impl #crate_path::macro_support::Bundle) {
                    #(#open_stmts)*
                    (Self { #(#sender_inits,)* }, #receiver_bundle)
                }

                #[allow(dead_code)]
                #vis fn open_with_waker(
                    waker: ::std::task::Waker,
                ) -> (Self, impl #crate_path::macro_support::Bundle) {
                    #(#open_waker_stmts)*
                    (Self { #(#sender_inits,)* }, #receiver_bundle)
                }

                #[allow(dead_code)]
                #vis fn add_channels(app: &mut #crate_path::macro_support::App) {
                    #add_plugins
                }
            }
        }
    }
}

fn chunk_tuple(items: Vec<TokenStream>) -> TokenStream {
    if items.is_empty() {
        return quote!(());
    }
    if items.len() <= 8 {
        return quote!((#(#items,)*));
    }
    let chunks = items
        .chunks(8)
        .map(|chunk| quote!((#(#chunk,)*)))
        .collect::<Vec<_>>();
    chunk_tuple(chunks)
}

struct Arguments {
    crate_path: Path,
}

impl Parse for Arguments {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let key: Ident = input.parse()?;
        if key != "crate" {
            return Err(syn::Error::new_spanned(key, "expected `crate = ...`"));
        }
        input.parse::<Token![=]>()?;
        let crate_path: Path = input.parse()?;
        if input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
        }
        if !input.is_empty() {
            return Err(input.error("unexpected trailing tokens"));
        }
        Ok(Self { crate_path })
    }
}

struct ChannelField {
    ident: Ident,
    kind: FieldKind,
}

enum FieldKind {
    Default,
    Spawn { payload: Type, manual: bool },
    Entity { payload: Type, manual: bool },
}

impl ChannelField {
    fn parse(field: &mut Field) -> syn::Result<Self> {
        let ident = field
            .ident
            .clone()
            .ok_or_else(|| syn::Error::new_spanned(&*field, "expected named field"))?;
        let options = FieldOptions::take(&mut field.attrs)?;
        if options.default {
            if options.manual {
                return Err(syn::Error::new_spanned(
                    &*field,
                    "`default` cannot be combined with `manual`",
                ));
            }
            return Ok(Self {
                ident,
                kind: FieldKind::Default,
            });
        }
        let (sender_name, payload) = extract_sender_payload(&field.ty)?;
        let kind = match sender_name.as_str() {
            "SpawnSender" => FieldKind::Spawn {
                payload,
                manual: options.manual,
            },
            "ChannelSender" => FieldKind::Entity {
                payload,
                manual: options.manual,
            },
            _ => {
                return Err(syn::Error::new_spanned(
                    &field.ty,
                    "expected `SpawnSender<T>`, `ChannelSender<T>`, or `#[channel(default)]`",
                ));
            }
        };
        Ok(Self { ident, kind })
    }
}

#[derive(Default)]
struct FieldOptions {
    manual: bool,
    default: bool,
}

impl FieldOptions {
    fn take(attrs: &mut Vec<Attribute>) -> syn::Result<Self> {
        let mut options = Self::default();
        let mut kept = Vec::with_capacity(attrs.len());
        for attr in attrs.drain(..) {
            if !attr.path().is_ident("channel") {
                kept.push(attr);
                continue;
            }
            let flags = attr.parse_args_with(Punctuated::<Ident, Token![,]>::parse_terminated)?;
            for flag in flags {
                if flag == "manual" {
                    options.manual = true;
                } else if flag == "default" {
                    options.default = true;
                } else {
                    return Err(syn::Error::new_spanned(
                        flag,
                        "expected `manual` or `default`",
                    ));
                }
            }
        }
        *attrs = kept;
        Ok(options)
    }
}

fn extract_sender_payload(ty: &Type) -> syn::Result<(String, Type)> {
    let Type::Path(type_path) = ty else {
        return Err(syn::Error::new_spanned(
            ty,
            "expected `SpawnSender<T>`, `ChannelSender<T>`, or `#[channel(default)]`",
        ));
    };
    let Some(segment) = type_path.path.segments.last() else {
        return Err(syn::Error::new_spanned(ty, "empty type path"));
    };
    let PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return Err(syn::Error::new_spanned(
            ty,
            "expected generic sender type like `SpawnSender<T>` or `ChannelSender<T>`",
        ));
    };
    if arguments.args.len() != 1 {
        return Err(syn::Error::new_spanned(
            arguments,
            "expected a single payload type argument",
        ));
    }
    let Some(GenericArgument::Type(payload)) = arguments.args.first() else {
        return Err(syn::Error::new_spanned(
            arguments,
            "expected a payload type argument",
        ));
    };
    Ok((segment.ident.to_string(), payload.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_quote;

    #[test]
    fn channels_macro_expands_spawn_entity_manual_and_default_fields() {
        let item: DeriveInput = parse_quote! {
            pub struct Group {
                #[channel(default)]
                counter: AtomicU64,
                spawns: SpawnSender<SpawnWork>,
                #[channel(manual)]
                ordered: ChannelSender<OrderedEvent>,
                status: ChannelSender<RunStatus>,
            }
        };
        let declaration = Declaration::parse(quote!(), item, "::vmux_channel").unwrap();
        let output = declaration.expand().to_string();
        assert!(output.contains("SpawnReceiver :: < SpawnWork > :: unbounded_with_waker"));
        assert!(output.contains("AsyncChannel :: < OrderedEvent > :: unbounded_with_waker"));
        assert!(output.contains("AsyncChannel :: < RunStatus > :: unbounded_with_waker"));
        assert!(output.contains("AsyncChannelPlugin :: spawn :: < SpawnWork > () . components ()"));
        assert!(output.contains("AsyncChannelPlugin :: < RunStatus > :: default ()"));
        assert!(!output.contains("AsyncChannelPlugin :: < OrderedEvent >"));
    }

    #[test]
    fn channels_macro_rejects_generics_and_unknown_field_types() {
        let generic: DeriveInput = parse_quote! {
            struct Bad<T> {
                items: SpawnSender<T>,
            }
        };
        assert!(Declaration::parse(quote!(), generic, "::vmux_channel").is_err());

        let unknown: DeriveInput = parse_quote! {
            struct Bad {
                plain: String,
            }
        };
        assert!(Declaration::parse(quote!(), unknown, "::vmux_channel").is_err());
    }
}
