use proc_macro2::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Data, DeriveInput, Expr, Fields, Ident, LitStr, Path, Token, Type};

struct Args {
    url: Expr,
    title: LitStr,
    component: Path,
    placement: Placement,
    document_url: Option<Expr>,
    dom_group: Option<LitStr>,
    root_id: LitStr,
    root_class: LitStr,
    head: LitStr,
    html_attributes: LitStr,
    body_class: LitStr,
    reports_title: bool,
    favicon: bool,
    transparent: bool,
    owns_subtree: bool,
    takes: Option<Type>,
    claims: Option<Type>,
}

#[derive(Clone, Copy)]
enum Placement {
    Layout,
    Pane,
    Modal,
}

impl Parse for Args {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut url = None;
        let mut title = None;
        let mut component = None;
        let mut placement = Placement::Pane;
        let mut document_url = None;
        let mut dom_group = None;
        let mut root_id = None;
        let mut root_class = None;
        let mut head = None;
        let mut html_attributes = None;
        let mut body_class = None;
        let mut reports_title = true;
        let mut favicon = true;
        let mut transparent = false;
        let mut owns_subtree = false;
        let mut takes = None;
        let mut claims = None;

        while !input.is_empty() {
            let key: Ident = input.parse()?;
            match key.to_string().as_str() {
                "subtree" => owns_subtree = true,
                "preserve_title" => reports_title = false,
                "no_favicon" => favicon = false,
                "transparent" => transparent = true,
                "url" => {
                    input.parse::<Token![=]>()?;
                    url = Some(input.parse()?);
                }
                "title" => {
                    input.parse::<Token![=]>()?;
                    title = Some(input.parse()?);
                }
                "component" => {
                    input.parse::<Token![=]>()?;
                    component = Some(input.parse()?);
                }
                "placement" => {
                    input.parse::<Token![=]>()?;
                    let value: Ident = input.parse()?;
                    placement = match value.to_string().as_str() {
                        "layout" => Placement::Layout,
                        "pane" => Placement::Pane,
                        "modal" => Placement::Modal,
                        _ => {
                            return Err(syn::Error::new_spanned(
                                value,
                                "placement must be layout, pane, or modal",
                            ));
                        }
                    };
                }
                "document_url" => {
                    input.parse::<Token![=]>()?;
                    document_url = Some(input.parse()?);
                }
                "dom_group" => {
                    input.parse::<Token![=]>()?;
                    dom_group = Some(input.parse()?);
                }
                "root_id" => {
                    input.parse::<Token![=]>()?;
                    root_id = Some(input.parse()?);
                }
                "root_class" => {
                    input.parse::<Token![=]>()?;
                    root_class = Some(input.parse()?);
                }
                "head" => {
                    input.parse::<Token![=]>()?;
                    head = Some(input.parse()?);
                }
                "html_attributes" => {
                    input.parse::<Token![=]>()?;
                    html_attributes = Some(input.parse()?);
                }
                "body_class" => {
                    input.parse::<Token![=]>()?;
                    body_class = Some(input.parse()?);
                }
                "takes" => {
                    input.parse::<Token![=]>()?;
                    takes = Some(input.parse()?);
                }
                "claims" => {
                    input.parse::<Token![=]>()?;
                    claims = Some(input.parse()?);
                }
                _ => return Err(syn::Error::new_spanned(key, "unknown page option")),
            }
            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }

        Ok(Self {
            url: url.ok_or_else(|| input.error("page requires url"))?,
            title: title.unwrap_or_else(|| LitStr::new("", proc_macro2::Span::call_site())),
            component: component.ok_or_else(|| input.error("page requires component"))?,
            placement,
            document_url,
            dom_group,
            root_id: root_id
                .unwrap_or_else(|| LitStr::new("main", proc_macro2::Span::call_site())),
            root_class: root_class.unwrap_or_else(|| {
                LitStr::new(
                    "flex min-h-0 min-w-0 flex-1 flex-col",
                    proc_macro2::Span::call_site(),
                )
            }),
            head: head.unwrap_or_else(|| {
                LitStr::new(
                    r#"<base href="/"/>
<style>
html, body { height: 100%; margin: 0; min-height: 0; }
body { display: flex; flex-direction: column; min-height: 0; overflow: hidden; }
</style>
<link rel="stylesheet" href="./assets/index.css"/>
<link rel="stylesheet" href="./assets/theme.css"/>"#,
                    proc_macro2::Span::call_site(),
                )
            }),
            html_attributes: html_attributes.unwrap_or_else(|| {
                LitStr::new(
                    r#"lang="en" class="h-full" style="color-scheme: light dark""#,
                    proc_macro2::Span::call_site(),
                )
            }),
            body_class: body_class.unwrap_or_else(|| {
                LitStr::new(
                    "m-0 flex h-full min-h-0 flex-col overflow-hidden p-0 text-foreground antialiased",
                    proc_macro2::Span::call_site(),
                )
            }),
            reports_title,
            favicon,
            transparent,
            owns_subtree,
            takes,
            claims,
        })
    }
}

pub(crate) fn expand(args: TokenStream, input: DeriveInput) -> syn::Result<TokenStream> {
    let args = syn::parse2::<Args>(args)?;
    let Data::Struct(data) = &input.data else {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "page requires a unit struct",
        ));
    };
    if !matches!(data.fields, Fields::Unit) {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "page requires a unit struct",
        ));
    }

    let ident = &input.ident;
    let url = args.url;
    let title = args.title;
    let component = args.component;
    let document_url = match args.document_url {
        Some(url) => quote! { ::core::option::Option::Some(#url) },
        None => quote! { ::core::option::Option::Some("vmux://start/") },
    };
    let dom_group = match args.dom_group {
        Some(group) => quote! { ::core::option::Option::Some(#group) },
        None => quote! { ::core::option::Option::None },
    };
    let root_id = args.root_id;
    let root_class = args.root_class;
    let head = args.head;
    let html_attributes = args.html_attributes;
    let body_class = args.body_class;
    let reports_title = args.reports_title;
    let favicon = args.favicon;
    let transparent = args.transparent;
    let owns_subtree = args.owns_subtree;
    let plugin = match args.placement {
        Placement::Layout => quote! { ::vmux_native::NativePagePlugin::as_layout(&Self::NATIVE) },
        Placement::Pane => quote! { ::vmux_native::NativePagePlugin::in_pane(&Self::NATIVE) },
        Placement::Modal => quote! { ::vmux_native::NativePagePlugin::as_modal(&Self::NATIVE) },
    };
    let plugin = match args.takes {
        Some(takes) => quote! { (#plugin).takes::<#takes>() },
        None => plugin,
    };
    let plugin = match args.claims {
        Some(claims) => quote! { (#plugin).claims::<#claims>() },
        None => plugin,
    };

    Ok(quote! {
        #input

        impl #ident {
            pub const URL: &'static str = #url;

            pub const NATIVE: ::vmux_native::NativePage = ::vmux_native::NativePage {
                url: Self::URL,
                document_url: #document_url,
                title: #title,
                reports_title: #reports_title,
                favicon: #favicon,
                component: #component,
                dom_group: #dom_group,
                root_id: #root_id,
                root_class: #root_class,
                head: #head,
                html_attributes: #html_attributes,
                body_class: #body_class,
                transparent: #transparent,
                owns_subtree: #owns_subtree,
            };

            pub fn plugin() -> ::vmux_native::NativePagePlugin {
                #plugin
            }
        }
    })
}
