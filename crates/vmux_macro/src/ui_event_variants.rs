use std::collections::HashSet;

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::{Data, DeriveInput, Field, Fields, Ident, LitInt, LitStr, Path, Token, parenthesized};

mod keyword {
    syn::custom_keyword!(any);
    syn::custom_keyword!(shared);
    syn::custom_keyword!(target);
    syn::custom_keyword!(targets);
    syn::custom_keyword!(version);
}

enum Target {
    Any,
    Host(LitStr),
    Hosts(Vec<LitStr>),
}

struct Args {
    derives: Vec<Path>,
    shared: Vec<Field>,
    target: Option<Target>,
    version: Option<LitInt>,
}

impl Parse for Args {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut derives = Vec::new();
        let mut shared = None;
        let mut target = None;
        let mut version = None;

        while !input.is_empty() {
            if input.peek(keyword::shared) {
                let key: keyword::shared = input.parse()?;
                if shared.is_some() {
                    return Err(syn::Error::new_spanned(key, "duplicate shared fields"));
                }
                let content;
                parenthesized!(content in input);
                let fields = content.parse_terminated(Field::parse_named, Token![,])?;
                shared = Some(fields.into_iter().collect());
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
                syn::bracketed!(content in input);
                let values = content
                    .parse_terminated(|input| input.parse::<LitStr>(), Token![,])?
                    .into_iter()
                    .collect();
                target = Some(Target::Hosts(values));
            } else {
                derives.push(input.parse()?);
            }
            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }

        Ok(Self {
            derives,
            shared: shared.unwrap_or_default(),
            target,
            version,
        })
    }
}

impl Args {
    fn event_args(&self) -> TokenStream {
        let mut args = self
            .derives
            .iter()
            .map(|derive| quote!(#derive))
            .collect::<Vec<_>>();
        if let Some(version) = &self.version {
            args.push(quote!(version = #version));
        }
        match &self.target {
            Some(Target::Any) => args.push(quote!(target = any)),
            Some(Target::Host(host)) => args.push(quote!(target = #host)),
            Some(Target::Hosts(hosts)) => args.push(quote!(targets = [#(#hosts),*])),
            None => {}
        }
        quote!(#(#args),*)
    }
}

pub(crate) fn expand(args: TokenStream, input: DeriveInput) -> syn::Result<TokenStream> {
    let args = syn::parse2::<Args>(args)?;
    if !input.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &input.generics,
            "ui_event_variants does not support generic enums",
        ));
    }
    let Data::Enum(data) = &input.data else {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "ui_event_variants only supports enums",
        ));
    };

    let operation_ident = &input.ident;
    let operation_name = operation_ident.to_string();
    let Some(stem) = operation_name.strip_suffix("Operation") else {
        return Err(syn::Error::new_spanned(
            operation_ident,
            "ui_event_variants enum name must end with Operation",
        ));
    };
    let visibility = &input.vis;
    let event_args = args.event_args();
    let shared_idents = field_idents(&args.shared)?;
    let mut requests = Vec::new();
    let mut request_idents = Vec::new();
    let mut send_arms = Vec::new();

    for variant in &data.variants {
        let variant_ident = &variant.ident;
        let request_ident = format_ident!("{}{}Request", stem, variant_ident);
        let fields = match &variant.fields {
            Fields::Unit => Vec::new(),
            Fields::Named(fields) => fields.named.iter().cloned().collect(),
            Fields::Unnamed(fields) => {
                return Err(syn::Error::new_spanned(
                    fields,
                    "ui_event_variants only supports unit and named variants",
                ));
            }
        };
        let field_idents = field_idents(&fields)?;
        ensure_unique_fields(&args.shared, &fields)?;
        let request_field_idents = shared_idents.iter().chain(field_idents.iter());
        let request_fields = args.shared.iter().chain(fields.iter()).map(public_field);
        let operation_fields = field_idents
            .iter()
            .map(|field| quote!(#field: self.#field.clone()));
        let operation = if field_idents.is_empty() {
            quote!(#operation_ident::#variant_ident)
        } else {
            quote!(#operation_ident::#variant_ident { #(#operation_fields),* })
        };
        let pattern = if field_idents.is_empty() {
            quote!(Self::#variant_ident)
        } else {
            quote!(Self::#variant_ident { #(#field_idents),* })
        };

        requests.push(quote! {
            #[vmux_api::ui_event(#event_args)]
            #visibility struct #request_ident {
                #(#request_fields),*
            }

            impl #request_ident {
                #visibility fn operation(&self) -> #operation_ident {
                    #operation
                }
            }
        });
        request_idents.push(request_ident.clone());
        send_arms.push(quote! {
            #pattern => ::vmux_ui::hooks::send(&#request_ident {
                #(#request_field_idents),*
            })
        });
    }

    let requests_ident = format_ident!("{}Requests", operation_ident);
    let shared_parameters = args.shared.iter().map(|field| {
        let ident = field.ident.as_ref().expect("named field");
        let ty = &field.ty;
        quote!(#ident: #ty)
    });
    let operation = crate::contract::expand_with_derives(input.clone(), args.derives.iter());

    Ok(quote! {
        #operation

        #(#requests)*

        #visibility type #requests_ident = (#(#request_idents,)*);

        #[cfg(ui)]
        impl #operation_ident {
            #visibility fn send(self, #(#shared_parameters),*) {
                let _ = match self {
                    #(#send_arms),*
                };
            }
        }
    })
}

fn field_idents(fields: &[Field]) -> syn::Result<Vec<Ident>> {
    fields
        .iter()
        .map(|field| {
            field
                .ident
                .clone()
                .ok_or_else(|| syn::Error::new_spanned(field, "expected named field"))
        })
        .collect()
}

fn ensure_unique_fields(shared: &[Field], fields: &[Field]) -> syn::Result<()> {
    let mut names = HashSet::new();
    for field in shared.iter().chain(fields) {
        let ident = field.ident.as_ref().expect("named field");
        if !names.insert(ident.to_string()) {
            return Err(syn::Error::new_spanned(ident, "duplicate request field"));
        }
    }
    Ok(())
}

fn public_field(field: &Field) -> TokenStream {
    let attrs = &field.attrs;
    let ident = field.ident.as_ref().expect("named field");
    let ty = &field.ty;
    quote!(#(#attrs)* pub #ident: #ty)
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::{Item, parse_quote, parse2};

    #[test]
    fn operation_variants_generate_individual_requests() {
        let input = parse_quote! {
            pub enum GitOperation {
                Amend,
                CheckoutCommit { commit: String },
            }
        };
        let output = expand(quote!(Eq, target = "git", shared(repo_root: String)), input).unwrap();
        let file = parse2::<syn::File>(output).unwrap();
        let names = file
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Struct(item) => Some(item.ident.to_string()),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(names, ["GitAmendRequest", "GitCheckoutCommitRequest"]);
    }
}
