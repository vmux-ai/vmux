use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Data, DeriveInput, Error, Fields, LitStr, Variant};

pub fn expand(input: DeriveInput) -> syn::Result<TokenStream> {
    let Data::Enum(routes) = &input.data else {
        return Err(Error::new_spanned(
            &input.ident,
            "a route is an enum of the places the app can be",
        ));
    };

    let route = &input.ident;
    let visibility = &input.vis;
    let named = format_ident!("{}Name", route);

    let mut cases = Vec::new();
    let mut names = Vec::new();
    let mut titles = Vec::new();
    let mut blank = quote! { ::std::option::Option::None };
    for variant in &routes.variants {
        let case = &variant.ident;
        if variant
            .attrs
            .iter()
            .any(|attr| attr.path().is_ident("blank"))
        {
            if !matches!(&variant.fields, Fields::Unnamed(fields) if fields.unnamed.len() == 1) {
                return Err(Error::new_spanned(
                    case,
                    "a blank route takes the tab's number, so it holds exactly one field",
                ));
            }
            blank = quote! { ::std::option::Option::Some(Self::#case(at)) };
        }
        cases.push(quote! { #case });
        names.push(match &variant.fields {
            Fields::Unit => quote! { Self::#case => #named::#case },
            Fields::Unnamed(_) => quote! { Self::#case(..) => #named::#case },
            Fields::Named(_) => quote! { Self::#case { .. } => #named::#case },
        });
        titles.push(title(variant)?);
    }

    Ok(quote! {
        #[derive(Clone, Copy, PartialEq, Eq, Debug)]
        #visibility enum #named {
            #(#cases,)*
        }

        impl ::vmux_mobile::nav::Route for #route {
            type Name = #named;

            fn name(&self) -> #named {
                match self {
                    #(#names,)*
                }
            }

            fn title(&self) -> ::std::string::String {
                match self {
                    #(#titles,)*
                }
            }

            fn blank(at: usize) -> ::std::option::Option<Self> {
                let _ = at;
                #blank
            }
        }
    })
}

fn title(variant: &Variant) -> syn::Result<TokenStream> {
    let case = &variant.ident;
    let Some(spelled) = spelling(variant)? else {
        let name = case.to_string();
        return Ok(match &variant.fields {
            Fields::Unit => quote! { Self::#case => #name.to_string() },
            Fields::Unnamed(_) => quote! { Self::#case(..) => #name.to_string() },
            Fields::Named(_) => quote! { Self::#case { .. } => #name.to_string() },
        });
    };

    Ok(match &variant.fields {
        Fields::Unit => quote! { Self::#case => format!(#spelled) },
        Fields::Unnamed(fields) => {
            let bound: Vec<_> = (0..fields.unnamed.len())
                .map(|at| format_ident!("at{at}"))
                .collect();
            quote! { Self::#case(#(#bound),*) => format!(#spelled, #(#bound),*) }
        }
        Fields::Named(fields) => {
            let bound: Vec<_> = fields
                .named
                .iter()
                .filter_map(|field| field.ident.clone())
                .collect();
            quote! { Self::#case { #(#bound),* } => format!(#spelled, #(#bound = #bound),*) }
        }
    })
}

fn spelling(variant: &Variant) -> syn::Result<Option<LitStr>> {
    for attribute in &variant.attrs {
        if !attribute.path().is_ident("route") {
            continue;
        }
        return attribute.parse_args::<LitStr>().map(Some);
    }
    Ok(None)
}
