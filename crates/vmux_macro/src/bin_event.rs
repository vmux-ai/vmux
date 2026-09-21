use proc_macro2::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{DeriveInput, Ident, LitInt, LitStr, Token, bracketed};

mod keyword {
    syn::custom_keyword!(any);
    syn::custom_keyword!(name);
    syn::custom_keyword!(namespace);
    syn::custom_keyword!(target);
    syn::custom_keyword!(targets);
    syn::custom_keyword!(version);
}

pub(crate) enum Direction {
    Host,
    Ui,
    Both,
}

enum Target {
    Any,
    Host(LitStr),
    Hosts(Vec<LitStr>),
}

struct Args {
    namespace: Option<LitStr>,
    name: LitStr,
    version: Option<LitInt>,
    target: Target,
}

impl Parse for Args {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut name: Option<LitStr> = None;
        let mut namespace: Option<LitStr> = None;
        let mut version: Option<LitInt> = None;
        let mut target: Option<Target> = None;

        while !input.is_empty() {
            if input.peek(keyword::namespace) {
                let key: keyword::namespace = input.parse()?;
                input.parse::<Token![=]>()?;
                if namespace.is_some() {
                    return Err(syn::Error::new_spanned(key, "duplicate namespace"));
                }
                namespace = Some(input.parse()?);
            } else if input.peek(keyword::name) {
                let key: keyword::name = input.parse()?;
                input.parse::<Token![=]>()?;
                if name.is_some() {
                    return Err(syn::Error::new_spanned(key, "duplicate name"));
                }
                name = Some(input.parse()?);
            } else if input.peek(keyword::version) {
                let key: keyword::version = input.parse()?;
                input.parse::<Token![=]>()?;
                if version.is_some() {
                    return Err(syn::Error::new_spanned(key, "duplicate version"));
                }
                version = Some(input.parse()?);
            } else if input.peek(keyword::target) {
                let key: keyword::target = input.parse()?;
                input.parse::<Token![=]>()?;
                if target.is_some() {
                    return Err(syn::Error::new_spanned(key, "duplicate target"));
                }
                target = if input.peek(keyword::any) {
                    input.parse::<keyword::any>()?;
                    Some(Target::Any)
                } else {
                    Some(Target::Host(input.parse()?))
                };
            } else if input.peek(keyword::targets) {
                let key: keyword::targets = input.parse()?;
                input.parse::<Token![=]>()?;
                if target.is_some() {
                    return Err(syn::Error::new_spanned(key, "duplicate target"));
                }
                let content;
                bracketed!(content in input);
                let values = content
                    .parse_terminated(|input| input.parse::<LitStr>(), Token![,])?
                    .into_iter()
                    .collect();
                target = Some(Target::Hosts(values));
            } else {
                let key: Ident = input.parse()?;
                return Err(syn::Error::new_spanned(key, "unknown bin event option"));
            }
            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }

        let name = name.ok_or_else(|| input.error("missing name"))?;
        let target = target.ok_or_else(|| syn::Error::new(name.span(), "missing target"))?;
        Ok(Self {
            namespace,
            name,
            version,
            target,
        })
    }
}

fn validate_id_component(label: &str, value: &LitStr) -> syn::Result<()> {
    let component = value.value();
    if component.is_empty() {
        return Err(syn::Error::new(value.span(), format!("empty {label}")));
    }
    if component.contains('.') || component.contains('@') {
        return Err(syn::Error::new(
            value.span(),
            format!("{label} cannot contain `.` or `@`"),
        ));
    }
    Ok(())
}

pub(crate) fn expand(
    args: TokenStream,
    input: DeriveInput,
    direction: Direction,
) -> syn::Result<TokenStream> {
    let Args {
        namespace,
        name,
        version,
        target,
    } = syn::parse2(args)?;
    validate_id_component("name", &name)?;
    if let Some(namespace) = &namespace {
        validate_id_component("namespace", namespace)?;
    }
    let ident = &input.ident;
    let generics = &input.generics;
    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();
    let version_value = version
        .as_ref()
        .map(LitInt::base10_parse::<u16>)
        .transpose()?
        .unwrap_or(1);
    let id = match &namespace {
        Some(namespace) => format!("{}.{}@{version_value}", namespace.value(), name.value()),
        None => format!("{}@{version_value}", name.value()),
    };
    let id = LitStr::new(&id, name.span());
    let version = version
        .map(|version| quote! { const VERSION: u16 = #version; })
        .unwrap_or_default();
    let namespace = namespace
        .map(|namespace| quote! { const NAMESPACE: &'static str = #namespace; })
        .unwrap_or_default();
    let target = match target {
        Target::Any => quote! { ::vmux_api::BinEventTarget::Any },
        Target::Host(host) => quote! { ::vmux_api::BinEventTarget::Host(#host) },
        Target::Hosts(hosts) => quote! { ::vmux_api::BinEventTarget::Hosts(&[#(#hosts),*]) },
    };
    let direction_impl = match direction {
        Direction::Host => quote! {
            impl #impl_generics ::vmux_api::HostEvent for #ident #type_generics #where_clause {}
        },
        Direction::Ui => quote! {
            impl #impl_generics ::vmux_api::UiEvent for #ident #type_generics #where_clause {}
        },
        Direction::Both => quote! {
            impl #impl_generics ::vmux_api::HostEvent for #ident #type_generics #where_clause {}
            impl #impl_generics ::vmux_api::UiEvent for #ident #type_generics #where_clause {}
        },
    };

    Ok(quote! {
        #input

        impl #impl_generics ::vmux_api::BinEvent for #ident #type_generics #where_clause {
            const ID: &'static str = #id;
            #namespace
            const NAME: &'static str = #name;
            #version
            const TARGET: ::vmux_api::BinEventTarget = #target;
        }

        #direction_impl
    })
}
