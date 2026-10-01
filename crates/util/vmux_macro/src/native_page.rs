use proc_macro2::{TokenStream, TokenTree};
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
    #[serde(default)]
    name: String,
    url: String,
    title: String,
    #[serde(default)]
    manifest_url: String,
    #[serde(default)]
    manifest_title: String,
    #[serde(default)]
    asset_host: String,
    #[serde(default)]
    owns_subtree: bool,
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
    #[serde(default)]
    startup: bool,
    #[serde(default)]
    manifest: bool,
    #[serde(default)]
    permissions: Vec<String>,
}

#[derive(Deserialize)]
struct FeatureManifestFile {
    pages: Vec<PageManifestFile>,
}

fn default_manifest_file() -> LitStr {
    let path = std::env::var_os("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .map(|root| root.join("src/feature.ron"));
    let file = if path.is_some_and(|path| path.exists()) {
        "src/feature.ron"
    } else {
        "src/page.ron"
    };
    LitStr::new(file, proc_macro2::Span::call_site())
}

impl PageManifestFile {
    fn read(file: &LitStr, page: Option<&LitStr>) -> syn::Result<Self> {
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
        if let Ok(manifest) = ron::from_str(&source) {
            return Ok(manifest);
        }
        let feature: FeatureManifestFile = ron::from_str(&source).map_err(|error| {
            syn::Error::new(
                file.span(),
                format!(
                    "failed to parse feature manifest {}: {error}",
                    path.display()
                ),
            )
        })?;
        let key = page
            .map(LitStr::value)
            .unwrap_or_else(|| "default".to_string());
        let mut selected = None;
        for manifest in feature.pages {
            if manifest.name != key {
                continue;
            }
            if selected.is_some() {
                return Err(syn::Error::new(
                    page.map(LitStr::span).unwrap_or_else(|| file.span()),
                    format!(
                        "feature manifest {} contains duplicate page {key}",
                        path.display()
                    ),
                ));
            }
            selected = Some(manifest);
        }
        if let Some(manifest) = selected {
            return Ok(manifest);
        }
        Err(syn::Error::new(
            page.map(LitStr::span).unwrap_or_else(|| file.span()),
            format!(
                "feature manifest {} has no page named {key}",
                path.display()
            ),
        ))
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

    fn manifest(&self, file: &LitStr) -> syn::Result<TokenStream> {
        let url = if self.manifest_url.is_empty() {
            &self.url
        } else {
            &self.manifest_url
        };
        let title = if self.manifest_title.is_empty() {
            &self.title
        } else {
            &self.manifest_title
        };
        let asset_host = if self.asset_host.is_empty() {
            url.strip_prefix("vmux://")
                .and_then(|url| url.split('/').next())
                .or_else(|| url.split_once("://").map(|(scheme, _)| scheme))
                .filter(|host| !host.is_empty())
                .ok_or_else(|| syn::Error::new(file.span(), "page manifest requires asset_host"))?
        } else {
            &self.asset_host
        };
        let url = LitStr::new(url, file.span());
        let title = LitStr::new(title, file.span());
        let asset_host = LitStr::new(asset_host, file.span());
        let owns_subtree = self.owns_subtree;
        let title_message_id = if self.title_message_id.is_empty() {
            quote! { ::core::option::Option::None }
        } else {
            let value = LitStr::new(&self.title_message_id, file.span());
            quote! { ::core::option::Option::Some(#value) }
        };
        let replaces_command = if self.replaces_command.is_empty() {
            quote! { ::core::option::Option::None }
        } else {
            let value = LitStr::new(&self.replaces_command, file.span());
            quote! { ::core::option::Option::Some(#value) }
        };
        let keywords = self
            .keywords
            .iter()
            .map(|value| LitStr::new(value, file.span()))
            .collect::<Vec<_>>();
        let icon = match self.icon(file)? {
            Some(value) => quote! { ::core::option::Option::Some(#value) },
            None => quote! { ::core::option::Option::None },
        };
        let command_bar = self.command_bar;
        let startup = self.startup;
        Ok(quote! {
            ::vmux_core::page::PageManifest {
                url: #url,
                asset_host: #asset_host,
                owns_subtree: #owns_subtree,
                title: #title,
                title_message_id: #title_message_id,
                replaces_command: #replaces_command,
                keywords: &[#(#keywords),*],
                icon: #icon,
                command_bar: #command_bar,
                startup: #startup,
            }
        })
    }
}

struct ManifestArgs {
    file: LitStr,
    page: Option<LitStr>,
}

impl Parse for ManifestArgs {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut file = None;
        let mut page = None;
        while !input.is_empty() {
            let key: Ident = input.parse()?;
            input.parse::<Token![=]>()?;
            match key.to_string().as_str() {
                "file" => file = Some(input.parse()?),
                "page" => page = Some(input.parse()?),
                _ => return Err(syn::Error::new_spanned(key, "unknown page manifest option")),
            }
            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }
        Ok(Self {
            file: file.unwrap_or_else(default_manifest_file),
            page,
        })
    }
}

fn expand_manifest(args: TokenStream, input: DeriveInput) -> syn::Result<TokenStream> {
    let args = syn::parse2::<ManifestArgs>(args)?;
    let page = PageManifestFile::read(&args.file, args.page.as_ref())?;
    let url = LitStr::new(&page.url, args.file.span());
    let manifest = page.manifest(&args.file)?;
    let ident = &input.ident;
    let file = &args.file;
    Ok(quote! {
        const _: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/", #file));

        #input

        impl #ident {
            pub const URL: &'static str = #url;
            pub const MANIFEST: ::vmux_core::page::PageManifest = #manifest;
        }
    })
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
    permissions: Vec<LitStr>,
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
        let mut page = None;
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
        let mut permissions = Vec::new();

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
                "page" => {
                    input.parse::<Token![=]>()?;
                    page = Some(input.parse()?);
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

        if file.is_none() && url.is_none() {
            file = Some(default_manifest_file());
        }

        if let Some(manifest_file) = file.as_ref() {
            let page_manifest = PageManifestFile::read(manifest_file, page.as_ref())?;
            if url.is_none() {
                let value = LitStr::new(&page_manifest.url, manifest_file.span());
                url = Some(parse_quote!(#value));
            }
            if title.is_none() {
                title = Some(LitStr::new(&page_manifest.title, manifest_file.span()));
            }
            if document_url.is_none() && !page_manifest.manifest_url.is_empty() {
                let value = LitStr::new(&page_manifest.manifest_url, manifest_file.span());
                document_url = Some(parse_quote!(#value));
            }
            if title_message_id.is_none() && !page_manifest.title_message_id.is_empty() {
                title_message_id = Some(LitStr::new(
                    &page_manifest.title_message_id,
                    manifest_file.span(),
                ));
            }
            if replaces_command.is_none() && !page_manifest.replaces_command.is_empty() {
                replaces_command = Some(LitStr::new(
                    &page_manifest.replaces_command,
                    manifest_file.span(),
                ));
            }
            if keywords.is_none() {
                let values = page_manifest
                    .keywords
                    .iter()
                    .map(|value| LitStr::new(value, manifest_file.span()))
                    .collect::<Vec<_>>();
                keywords = Some(parse_quote!([#(#values),*]));
            }
            if icon.is_none() {
                icon = page_manifest.icon(manifest_file)?;
            }
            command_bar |= page_manifest.command_bar;
            manifest |= page_manifest.manifest;
            permissions = page_manifest
                .permissions
                .iter()
                .map(|permission| LitStr::new(permission, manifest_file.span()))
                .collect();
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
            permissions,
        })
    }
}

pub(crate) fn expand(args: TokenStream, input: DeriveInput) -> syn::Result<TokenStream> {
    let native = args
        .clone()
        .into_iter()
        .any(|token| matches!(token, TokenTree::Ident(ident) if ident == "component"));
    if native {
        expand_native(args, input)
    } else {
        expand_manifest(args, input)
    }
}

fn expand_native(args: TokenStream, input: DeriveInput) -> syn::Result<TokenStream> {
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
    let placement = args.placement;
    let url = args.url;
    let title = args.title;
    let component = args.component;
    let document_url = match args.document_url {
        Some(url) => quote! { ::core::option::Option::Some(#url) },
        None => quote! { ::core::option::Option::None },
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
    let permissions = args.permissions;
    let plugin = match placement {
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
                    url: #url,
                    asset_host: #host,
                    owns_subtree: #owns_subtree,
                    title: #title,
                    title_message_id: #title_message_id,
                    replaces_command: #replaces_command,
                    keywords: #keywords,
                icon: #icon,
                command_bar: #command_bar,
                startup: false,
            };
        }
    });

    Ok(quote! {
        #manifest_dependency

        #[cfg_attr(not(host), allow(dead_code))]
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
                permissions: &[#(#permissions),*],
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
                manifest: true,
                permissions: ["ToolsUiState", "ToolsRefreshRequest"],
            )"#,
        )
        .unwrap();

        assert_eq!(manifest.url, "vmux://tools/");
        assert_eq!(manifest.title, "Tools");
        assert_eq!(manifest.title_message_id, "tools-title");
        assert_eq!(manifest.keywords, ["tools", "mcp"]);
        assert_eq!(manifest.icon, "Hammer");
        assert!(manifest.command_bar);
        assert!(manifest.manifest);
        assert_eq!(
            manifest.permissions,
            ["ToolsUiState", "ToolsRefreshRequest"]
        );
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

    #[test]
    fn feature_manifest_file_parses_named_pages() {
        let manifest = ron::from_str::<FeatureManifestFile>(
            r#"(
                pages: [
                    (name: "default", url: "vmux://tools/", title: "Tools"),
                    (name: "detail", url: "vmux://tools/detail", title: "Detail"),
                ],
            )"#,
        )
        .unwrap();

        assert_eq!(manifest.pages[0].title, "Tools");
        assert_eq!(manifest.pages[1].title, "Detail");
    }
}
