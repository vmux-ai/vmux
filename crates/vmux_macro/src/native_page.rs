use proc_macro2::TokenStream;
use quote::quote;
use serde::Deserialize;
use std::path::PathBuf;
use syn::parse::{Parse, ParseStream};
use syn::{
    Data, DeriveInput, Expr, ExprArray, Fields, Ident, LitStr, Path, Token, Type, parse_quote,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PageManifestFile {
    url: String,
    title: String,
    #[serde(default)]
    title_message_id: String,
    #[serde(default)]
    replaces_command: String,
    #[serde(default)]
    keywords: Vec<String>,
    #[serde(default)]
    icon: String,
    #[serde(default)]
    command_bar: bool,
}

impl PageManifestFile {
    fn read(file: &LitStr) -> syn::Result<Self> {
        let crate_root = std::env::var_os("CARGO_MANIFEST_DIR")
            .map(PathBuf::from)
            .ok_or_else(|| syn::Error::new(file.span(), "CARGO_MANIFEST_DIR is not set"))?;
        let path = crate_root.join(file.value());
        let source = std::fs::read_to_string(&path).map_err(|error| {
            syn::Error::new(
                file.span(),
                format!("failed to read page manifest {}: {error}", path.display()),
            )
        })?;
        ron::from_str(&source).map_err(|error| {
            syn::Error::new(
                file.span(),
                format!("failed to parse page manifest {}: {error}", path.display()),
            )
        })
    }

    fn icon(&self, file: &LitStr) -> syn::Result<Option<Expr>> {
        if self.icon.is_empty() {
            return Ok(None);
        }
        let icon = syn::parse_str::<Ident>(&self.icon).map_err(|error| {
            syn::Error::new(
                file.span(),
                format!("page manifest icon must be a BuiltinIcon variant: {error}"),
            )
        })?;
        Ok(Some(parse_quote!(::vmux_core::BuiltinIcon::#icon)))
    }
}

struct Args {
    file: Option<LitStr>,
    url: Expr,
    title: LitStr,
    component: Path,
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
    claims: Option<Type>,
    manifest: bool,
    title_message_id: Option<LitStr>,
    replaces_command: Option<LitStr>,
    keywords: Option<ExprArray>,
    icon: Option<Expr>,
    command_bar: bool,
}

#[derive(Clone, Copy)]
enum Placement {
    Layout,
    Pane,
    Modal,
}

impl Parse for Args {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut file = None;
        let mut url = None;
        let mut title = None;
        let mut component = None;
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
        let mut claims = None;
        let mut manifest = false;
        let mut title_message_id = None;
        let mut replaces_command = None;
        let mut keywords = None;
        let mut icon = None;
        let mut command_bar = false;

        while !input.is_empty() {
            let key: Ident = input.parse()?;
            match key.to_string().as_str() {
                "subtree" => owns_subtree = true,
                "preserve_title" => reports_title = false,
                "no_favicon" => favicon = false,
                "transparent" => transparent = true,
                "manifest" => manifest = true,
                "command_bar" => command_bar = true,
                "file" => {
                    input.parse::<Token![=]>()?;
                    file = Some(input.parse()?);
                }
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
                "stylesheet" => {
                    input.parse::<Token![=]>()?;
                    stylesheet = Some(input.parse()?);
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
                "title_message_id" => {
                    input.parse::<Token![=]>()?;
                    title_message_id = Some(input.parse()?);
                }
                "replaces_command" => {
                    input.parse::<Token![=]>()?;
                    replaces_command = Some(input.parse()?);
                }
                "keywords" => {
                    input.parse::<Token![=]>()?;
                    keywords = Some(input.parse()?);
                }
                "icon" => {
                    input.parse::<Token![=]>()?;
                    icon = Some(input.parse()?);
                }
                _ => return Err(syn::Error::new_spanned(key, "unknown page option")),
            }
            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }

        if let Some(manifest_file) = file.as_ref() {
            let page = PageManifestFile::read(manifest_file)?;
            if url.is_none() {
                let value = LitStr::new(&page.url, manifest_file.span());
                url = Some(parse_quote!(#value));
            }
            if title.is_none() {
                title = Some(LitStr::new(&page.title, manifest_file.span()));
            }
            if title_message_id.is_none() && !page.title_message_id.is_empty() {
                title_message_id = Some(LitStr::new(&page.title_message_id, manifest_file.span()));
            }
            if replaces_command.is_none() && !page.replaces_command.is_empty() {
                replaces_command = Some(LitStr::new(&page.replaces_command, manifest_file.span()));
            }
            if keywords.is_none() {
                let values = page
                    .keywords
                    .iter()
                    .map(|value| LitStr::new(value, manifest_file.span()))
                    .collect::<Vec<_>>();
                keywords = Some(parse_quote!([#(#values),*]));
            }
            if icon.is_none() {
                icon = page.icon(manifest_file)?;
            }
            command_bar |= page.command_bar;
            manifest = true;
        }

        Ok(Self {
            file,
            url: url.ok_or_else(|| input.error("page requires url or file"))?,
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
            stylesheet: stylesheet.unwrap_or_else(|| {
                LitStr::new(
                    "./assets/index.css",
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
            manifest,
            title_message_id,
            replaces_command,
            keywords,
            icon,
            command_bar,
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
    let manifest_dependency = args.file.as_ref().map(|file| {
        quote! {
            const _: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/", #file));
        }
    });
    let manifest_host = if args.manifest {
        let Expr::Lit(url) = &args.url else {
            return Err(syn::Error::new_spanned(
                &args.url,
                "page manifest requires a literal vmux:// URL",
            ));
        };
        let syn::Lit::Str(url) = &url.lit else {
            return Err(syn::Error::new_spanned(
                &url.lit,
                "page manifest requires a literal vmux:// URL",
            ));
        };
        let value = url.value();
        let Some(host) = value
            .strip_prefix("vmux://")
            .and_then(|url| url.split('/').next())
            .filter(|host| !host.is_empty())
        else {
            return Err(syn::Error::new_spanned(
                url,
                "page manifest requires a vmux:// URL with a host",
            ));
        };
        Some(LitStr::new(host, url.span()))
    } else {
        None
    };
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
    let stylesheet = args.stylesheet;
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
    let manifest = manifest_host.map(|host| {
        let title_message_id = match args.title_message_id {
            Some(value) => quote! { ::core::option::Option::Some(#value) },
            None => quote! { ::core::option::Option::None },
        };
        let replaces_command = match args.replaces_command {
            Some(value) => quote! { ::core::option::Option::Some(#value) },
            None => quote! { ::core::option::Option::None },
        };
        let keywords = match args.keywords {
            Some(value) => quote! { &#value },
            None => quote! { &[] },
        };
        let icon = match args.icon {
            Some(value) => quote! { ::core::option::Option::Some(#value) },
            None => quote! { ::core::option::Option::None },
        };
        let command_bar = args.command_bar;
        quote! {
            #[cfg(host)]
            pub const MANIFEST: ::vmux_core::page::PageManifest =
                ::vmux_core::page::PageManifest {
                    host: #host,
                    title: #title,
                    title_message_id: #title_message_id,
                    replaces_command: #replaces_command,
                    keywords: #keywords,
                    icon: #icon,
                    command_bar: #command_bar,
                };
        }
    });

    Ok(quote! {
        #manifest_dependency

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
                stylesheet: #stylesheet,
                html_attributes: #html_attributes,
                body_class: #body_class,
                transparent: #transparent,
                owns_subtree: #owns_subtree,
            };

            #manifest

            #[cfg(host)]
            pub fn plugin() -> ::vmux_native::NativePagePlugin {
                #plugin
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_manifest_file_parses_static_metadata() {
        let manifest = ron::from_str::<PageManifestFile>(
            r#"(
                url: "vmux://tools/",
                title: "Tools",
                title_message_id: "tools-title",
                keywords: ["tools", "mcp"],
                icon: "Hammer",
                command_bar: true,
            )"#,
        )
        .unwrap();

        assert_eq!(manifest.url, "vmux://tools/");
        assert_eq!(manifest.title, "Tools");
        assert_eq!(manifest.title_message_id, "tools-title");
        assert_eq!(manifest.keywords, ["tools", "mcp"]);
        assert_eq!(manifest.icon, "Hammer");
        assert!(manifest.command_bar);
    }

    #[test]
    fn page_manifest_file_rejects_unknown_metadata() {
        assert!(
            ron::from_str::<PageManifestFile>(
                r#"(
                    url: "vmux://tools/",
                    title: "Tools",
                    unknown: true,
                )"#,
            )
            .is_err()
        );
    }
}
