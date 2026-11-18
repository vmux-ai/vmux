use proc_macro2::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{
    Data, DeriveInput, Expr, Fields, Ident, LitStr, Path, Token, Type, bracketed, parse_quote,
};

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
    args.expand(input)
}

struct Args {
    owner: Path,
    selector: Option<LitStr>,
    component: Option<Path>,
    states: Option<Vec<Path>>,
    events: Vec<Path>,
    placement: Placement,
    document_url: Option<Expr>,
    dom_group: Option<LitStr>,
    root_id: LitStr,
    root_class: LitStr,
    stylesheet: LitStr,
    html_attributes: LitStr,
    body_class: LitStr,
    reports_title: bool,
    favicon: bool,
    transparent: bool,
    owns_subtree: bool,
    takes: Option<Type>,
    reactive_takes: bool,
    claims: Option<Type>,
}

impl Parse for Args {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut owner = None;
        let mut selector = None;
        let mut component = None;
        let mut states = None;
        let mut events = Vec::new();
        let mut placement = Placement::Pane;
        let mut document_url = None;
        let mut dom_group = None;
        let mut root_id = None;
        let mut root_class = None;
        let mut stylesheet = None;
        let mut html_attributes = None;
        let mut body_class = None;
        let mut reports_title = true;
        let mut favicon = true;
        let mut transparent = false;
        let mut owns_subtree = false;
        let mut takes = None;
        let mut reactive_takes = false;
        let mut claims = None;
        while !input.is_empty() {
            let key: Ident = input.parse()?;
            match key.to_string().as_str() {
                "subtree" => owns_subtree = true,
                "preserve_title" => reports_title = false,
                "no_favicon" => favicon = false,
                "transparent" => transparent = true,
                "reactive_takes" => reactive_takes = true,
                "states" => {
                    states = Some(Self::paths(input)?);
                }
                "events" => {
                    events = Self::paths(input)?;
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
                _ => {
                    input.parse::<Token![=]>()?;
                    match key.to_string().as_str() {
                        "owner" => owner = Some(input.parse()?),
                        "page" => selector = Some(input.parse()?),
                        "component" => component = Some(input.parse()?),
                        "document_url" => document_url = Some(input.parse()?),
                        "dom_group" => dom_group = Some(input.parse()?),
                        "root_id" => root_id = Some(input.parse()?),
                        "root_class" => root_class = Some(input.parse()?),
                        "stylesheet" => stylesheet = Some(input.parse()?),
                        "html_attributes" => html_attributes = Some(input.parse()?),
                        "body_class" => body_class = Some(input.parse()?),
                        "takes" => takes = Some(input.parse()?),
                        "claims" => claims = Some(input.parse()?),
                        _ => return Err(syn::Error::new_spanned(key, "unknown page option")),
                    }
                }
            }
            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }
        if component.is_some() && states.is_none() {
            return Err(input.error("page requires states = [...]"));
        }
        if reactive_takes && takes.is_none() {
            return Err(input.error("reactive_takes requires takes = Type"));
        }
        Ok(Self {
            owner: owner.unwrap_or_else(|| parse_quote!(crate::Feature)), selector, component, states, events, placement,
            document_url, dom_group,
            root_id: root_id.unwrap_or_else(|| LitStr::new("main", input.span())),
            root_class: root_class.unwrap_or_else(|| LitStr::new("flex min-h-0 min-w-0 flex-1 flex-col", input.span())),
            stylesheet: stylesheet.unwrap_or_else(|| LitStr::new("./assets/index.css", input.span())),
            html_attributes: html_attributes.unwrap_or_else(|| LitStr::new(r#"lang="en" class="h-full""#, input.span())),
            body_class: body_class.unwrap_or_else(|| LitStr::new("m-0 flex h-full min-h-0 flex-col overflow-hidden bg-background p-0 text-foreground antialiased", input.span())),
            reports_title, favicon, transparent, owns_subtree, takes, reactive_takes, claims,
        })
    }
}

impl Args {
    fn paths(input: ParseStream<'_>) -> syn::Result<Vec<Path>> {
        input.parse::<Token![=]>()?;
        let content;
        bracketed!(content in input);
        Ok(Punctuated::<Path, Token![,]>::parse_terminated(&content)?
            .into_iter()
            .collect())
    }

    fn expand(self, input: DeriveInput) -> syn::Result<TokenStream> {
        let ident = &input.ident;
        let owner = self.owner;
        let selector = match self.selector {
            Some(selector) => quote!(Some(#selector)),
            None => quote!(None),
        };
        let states = self.states.unwrap_or_default();
        let events = self.events;
        let permissions: Vec<_> = states
            .iter()
            .chain(&events)
            .map(|state| quote!(<#state as ::vmux_api::BinEvent>::PERMISSION))
            .collect();
        let common = quote! {
            pub fn url() -> &'static str {
                Self::__vmux_binding().declaration().expect("valid page selector").url.as_str()
            }

            fn __vmux_binding() -> ::vmux_feature::FeaturePageBinding<#owner> {
                #(Self::__vmux_state::<#states>();)*
                #(Self::__vmux_event::<#events>();)*
                ::vmux_feature::FeaturePageBinding::new(#selector)
            }

            fn __vmux_state<S: ::vmux_api::UiState>() {}
            fn __vmux_event<E: ::vmux_api::HostEvent>() {}

            #[cfg(host)]
            pub fn manifest() -> ::vmux_ecs::page::PageManifest {
                *Self::__vmux_binding().manifest().expect("valid page manifest")
            }
        };
        let native = if let Some(component) = self.component {
            let document_url = match self.document_url {
                Some(url) => quote!(Some(#url)),
                None => quote!(None),
            };
            let dom_group = match self.dom_group {
                Some(group) => quote!(Some(#group)),
                None => quote!(None),
            };
            let root_id = self.root_id;
            let root_class = self.root_class;
            let stylesheet = self.stylesheet;
            let html_attributes = self.html_attributes;
            let body_class = self.body_class;
            let reports_title = self.reports_title;
            let favicon = self.favicon;
            let transparent = self.transparent;
            let owns_subtree = self.owns_subtree;
            let plugin = match self.placement {
                Placement::Layout => quote!(::vmux_page::PagePlugin::as_layout(Self::page())),
                Placement::Pane => quote!(::vmux_page::PagePlugin::in_pane(Self::page())),
                Placement::Modal => quote!(::vmux_page::PagePlugin::as_modal(Self::page())),
            };
            let plugin = match (self.takes, self.reactive_takes) {
                (Some(takes), true) => quote!((#plugin).takes_reactive::<#takes>()),
                (Some(takes), false) => quote!((#plugin).takes::<#takes>()),
                _ => plugin,
            };
            let plugin = match self.claims {
                Some(claims) => quote!((#plugin).claims::<#claims>()),
                None => plugin,
            };
            quote! {
                pub fn page() -> &'static ::vmux_page::Page {
                    static PAGE: ::std::sync::OnceLock<::vmux_page::Page> = ::std::sync::OnceLock::new();
                    PAGE.get_or_init(|| {
                        Self::__vmux_binding().page(::vmux_page::Page {
                            url: "", title: "", document_url: #document_url, reports_title: #reports_title,
                            favicon: #favicon, component: #component, dom_group: #dom_group,
                            root_id: #root_id, root_class: #root_class, stylesheet: #stylesheet,
                            html_attributes: #html_attributes, body_class: #body_class,
                            transparent: #transparent, owns_subtree: #owns_subtree, permissions: &[],
                        }, &[#(#permissions),*]).expect("valid page binding")
                    })
                }

                #[cfg(host)]
                pub fn plugin() -> ::vmux_page::PagePlugin { #plugin }
            }
        } else {
            TokenStream::new()
        };
        Ok(quote! {
            #[cfg_attr(not(host), allow(dead_code))]
            #input

            impl #ident { #common #native }

            ::vmux_feature::__private::inventory::submit! {
                ::vmux_feature::PageBindingRegistration::of::<#owner>(#selector, &[#(#permissions),*])
            }
        })
    }
}

enum Placement {
    Layout,
    Pane,
    Modal,
}
