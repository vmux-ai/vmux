use heck::{ToSnakeCase, ToTitleCase};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{Attribute, Data, DeriveInput, Fields, Ident, LitBool, LitStr, Path, Token, Variant};

pub(crate) struct Declaration {
    item: DeriveInput,
    crate_path: Path,
    standard_edit: Option<String>,
    submenus: Vec<SubmenuNode>,
    variants: Vec<VariantSpec>,
}

impl Declaration {
    pub(crate) fn parse(
        args: TokenStream,
        mut item: DeriveInput,
        default_crate: &str,
    ) -> syn::Result<Self> {
        let parsed_args = if args.is_empty() {
            Arguments::default()
        } else {
            syn::parse2::<Arguments>(args)?
        };
        let crate_path = match parsed_args.crate_path {
            Some(path) => path,
            None => syn::parse_str::<Path>(default_crate)?,
        };
        if !item.generics.params.is_empty() {
            return Err(syn::Error::new_spanned(
                &item.generics,
                "menu enum cannot have generic parameters",
            ));
        }
        let enum_prefix = item.ident.to_string().to_snake_case();
        let Data::Enum(data) = &mut item.data else {
            return Err(syn::Error::new_spanned(
                &item,
                "menu attribute requires a unit-variant enum",
            ));
        };
        if data.variants.is_empty() {
            return Err(syn::Error::new_spanned(
                &item,
                "menu enum must declare at least one variant",
            ));
        }
        let mut submenus = Vec::new();
        let mut variants = Vec::new();
        let mut active_path: Option<Vec<String>> = None;
        for variant in &mut data.variants {
            let parsed = ParsedVariant::parse(variant, &enum_prefix)?;
            for entry in parsed.submenus {
                match entry {
                    SubmenuAttr::StandardEdit(title) => {
                        submenus.push(SubmenuNode::StandardEdit(title));
                    }
                    SubmenuAttr::Path(path) => {
                        ensure_submenu_path(&mut submenus, &path);
                        active_path = Some(path);
                    }
                }
            }
            let Some(path) = active_path.as_ref() else {
                return Err(syn::Error::new_spanned(
                    &variant.ident,
                    "menu variant must follow a `#[submenu(\"...\")]` attribute",
                ));
            };
            let target = find_submenu_mut(&mut submenus, path)
                .ok_or_else(|| syn::Error::new_spanned(&variant.ident, "missing submenu path"))?;
            for node in parsed.leading {
                target.push(node);
            }
            target.push(MenuNode::Item(parsed.spec.clone()));
            variants.push(parsed.spec);
        }
        Ok(Self {
            item,
            crate_path,
            standard_edit: parsed_args.standard_edit,
            submenus,
            variants,
        })
    }

    pub(crate) fn expand(&self) -> TokenStream {
        let item = &self.item;
        let name = &item.ident;
        let crate_path = &self.crate_path;

        let id_arms = self.variants.iter().map(|spec| {
            let ident = &spec.ident;
            let id = &spec.id;
            quote!(Self::#ident => #id)
        });

        let mut stmts = Vec::new();
        let mut counter = 0usize;
        for submenu in &self.submenus {
            emit_submenu(
                submenu,
                &format_ident!("bar"),
                crate_path,
                name,
                &mut counter,
                &mut stmts,
            );
        }
        if let Some(title) = &self.standard_edit {
            stmts.push(quote! {
                commands.spawn((
                    #crate_path::Submenu::standard_edit(#title),
                    #crate_path::macro_support::ChildOf(bar),
                ));
            });
        }

        quote! {
            #[derive(#crate_path::macro_support::Component)]
            #item

            impl #crate_path::MenuAction for #name {
                fn id(self) -> &'static str {
                    match self {
                        #(#id_arms,)*
                    }
                }

                fn spawn(
                    commands: &mut #crate_path::macro_support::Commands,
                    mut about: ::core::option::Option<#crate_path::About>,
                ) -> #crate_path::macro_support::Entity {
                    let bar = commands.spawn(#crate_path::MenuBar).id();
                    #(#stmts)*
                    let _ = &mut about;
                    bar
                }
            }
        }
    }
}

fn emit_submenu(
    node: &SubmenuNode,
    parent: &Ident,
    crate_path: &Path,
    enum_name: &Ident,
    counter: &mut usize,
    stmts: &mut Vec<TokenStream>,
) {
    match node {
        SubmenuNode::StandardEdit(title) => {
            stmts.push(quote! {
                commands.spawn((
                    #crate_path::Submenu::standard_edit(#title),
                    #crate_path::macro_support::ChildOf(#parent),
                ));
            });
        }
        SubmenuNode::Group { title, children } => {
            let ident = format_ident!("submenu_{}", *counter);
            *counter += 1;
            stmts.push(quote! {
                let #ident = commands
                    .spawn((
                        #crate_path::Submenu::new(#title),
                        #crate_path::macro_support::ChildOf(#parent),
                    ))
                    .id();
            });
            for child in children {
                match child {
                    MenuNode::Submenu(nested) => {
                        emit_submenu(nested, &ident, crate_path, enum_name, counter, stmts);
                    }
                    MenuNode::Separator => {
                        stmts.push(quote! {
                            commands.spawn((
                                #crate_path::Separator,
                                #crate_path::macro_support::ChildOf(#ident),
                            ));
                        });
                    }
                    MenuNode::Predefined(kind) => {
                        let variant = format_ident!("{kind}");
                        if *kind == "About" {
                            stmts.push(quote! {
                                let about_entity = commands
                                    .spawn((
                                        #crate_path::Predefined::About,
                                        #crate_path::macro_support::ChildOf(#ident),
                                    ))
                                    .id();
                                if let ::core::option::Option::Some(metadata) = about.take() {
                                    commands.entity(about_entity).insert(metadata);
                                }
                            });
                        } else {
                            stmts.push(quote! {
                                commands.spawn((
                                    #crate_path::Predefined::#variant,
                                    #crate_path::macro_support::ChildOf(#ident),
                                ));
                            });
                        }
                    }
                    MenuNode::Item(spec) => {
                        let variant = &spec.ident;
                        let label = &spec.label;
                        let enabled = spec.enabled;
                        let id = &spec.id;
                        let accel = match &spec.accelerator {
                            Some(value) => quote!(.with_accelerator(#value)),
                            None => quote!(),
                        };
                        stmts.push(quote! {
                            commands.spawn((
                                #crate_path::Entry::new(#label, #enabled)
                                    .with_id(#id)
                                    #accel,
                                #enum_name::#variant,
                                #crate_path::macro_support::ChildOf(#ident),
                            ));
                        });
                    }
                }
            }
        }
    }
}

fn ensure_submenu_path(roots: &mut Vec<SubmenuNode>, path: &[String]) {
    let Some((first, rest)) = path.split_first() else {
        return;
    };
    let index = roots
        .iter()
        .position(|node| matches!(node, SubmenuNode::Group { title, .. } if title == first));
    let index = match index {
        Some(existing) => existing,
        None => {
            roots.push(SubmenuNode::Group {
                title: first.clone(),
                children: Vec::new(),
            });
            roots.len() - 1
        }
    };
    if let SubmenuNode::Group { children, .. } = &mut roots[index] {
        ensure_nested_submenu(children, rest);
    }
}

fn ensure_nested_submenu(children: &mut Vec<MenuNode>, path: &[String]) {
    let Some((first, rest)) = path.split_first() else {
        return;
    };
    let index = children.iter().position(|node| {
        matches!(
            node,
            MenuNode::Submenu(SubmenuNode::Group { title, .. }) if title == first
        )
    });
    let index = match index {
        Some(existing) => existing,
        None => {
            children.push(MenuNode::Submenu(SubmenuNode::Group {
                title: first.clone(),
                children: Vec::new(),
            }));
            children.len() - 1
        }
    };
    if let MenuNode::Submenu(SubmenuNode::Group { children, .. }) = &mut children[index] {
        ensure_nested_submenu(children, rest);
    }
}

fn find_submenu_mut<'a>(
    roots: &'a mut [SubmenuNode],
    path: &[String],
) -> Option<&'a mut Vec<MenuNode>> {
    let (first, rest) = path.split_first()?;
    let root = roots
        .iter_mut()
        .find(|node| matches!(node, SubmenuNode::Group { title, .. } if title == first))?;
    let SubmenuNode::Group { children, .. } = root else {
        return None;
    };
    find_nested_mut(children, rest)
}

fn find_nested_mut<'a>(
    children: &'a mut Vec<MenuNode>,
    path: &[String],
) -> Option<&'a mut Vec<MenuNode>> {
    let Some((first, rest)) = path.split_first() else {
        return Some(children);
    };
    let child = children.iter_mut().find(|node| {
        matches!(
            node,
            MenuNode::Submenu(SubmenuNode::Group { title, .. }) if title == first
        )
    })?;
    let MenuNode::Submenu(SubmenuNode::Group { children, .. }) = child else {
        return None;
    };
    find_nested_mut(children, rest)
}

#[derive(Default)]
struct Arguments {
    crate_path: Option<Path>,
    standard_edit: Option<String>,
}

impl Parse for Arguments {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut parsed = Self::default();
        while !input.is_empty() {
            if input.peek(Token![crate]) {
                input.parse::<Token![crate]>()?;
                input.parse::<Token![=]>()?;
                parsed.crate_path = Some(input.parse()?);
            } else {
                let key: Ident = input.parse()?;
                if key == "standard_edit" {
                    if input.peek(Token![=]) {
                        input.parse::<Token![=]>()?;
                        let title: LitStr = input.parse()?;
                        parsed.standard_edit = Some(title.value());
                    } else {
                        parsed.standard_edit = Some("Edit".to_string());
                    }
                } else {
                    return Err(syn::Error::new_spanned(
                        key,
                        "expected `crate = ...` or `standard_edit`",
                    ));
                }
            }
            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
        }
        Ok(parsed)
    }
}

enum SubmenuNode {
    StandardEdit(String),
    Group {
        title: String,
        children: Vec<MenuNode>,
    },
}

enum MenuNode {
    Submenu(SubmenuNode),
    Separator,
    Predefined(&'static str),
    Item(VariantSpec),
}

enum SubmenuAttr {
    Path(Vec<String>),
    StandardEdit(String),
}

#[derive(Clone)]
struct VariantSpec {
    ident: Ident,
    id: String,
    label: String,
    enabled: bool,
    accelerator: Option<String>,
}

struct ParsedVariant {
    submenus: Vec<SubmenuAttr>,
    leading: Vec<MenuNode>,
    spec: VariantSpec,
}

impl ParsedVariant {
    fn parse(variant: &mut Variant, enum_prefix: &str) -> syn::Result<Self> {
        if !matches!(variant.fields, Fields::Unit) {
            return Err(syn::Error::new_spanned(
                &variant.fields,
                "menu variants must be unit variants",
            ));
        }
        let mut submenus = Vec::new();
        let mut leading = Vec::new();
        let mut item_options: Option<ItemOptions> = None;
        let mut kept = Vec::with_capacity(variant.attrs.len());
        for attr in variant.attrs.drain(..) {
            if attr.path().is_ident("submenu") {
                submenus.push(parse_submenu_attr(&attr)?);
            } else if attr.path().is_ident("separator") {
                if !matches!(attr.meta, syn::Meta::Path(_)) {
                    return Err(syn::Error::new_spanned(
                        attr,
                        "`#[separator]` takes no arguments",
                    ));
                }
                leading.push(MenuNode::Separator);
            } else if attr.path().is_ident("predefined") {
                leading.push(MenuNode::Predefined(parse_predefined_attr(&attr)?));
            } else if attr.path().is_ident("item") {
                item_options = Some(attr.parse_args::<ItemOptions>()?);
            } else {
                kept.push(attr);
            }
        }
        variant.attrs = kept;
        let ident = variant.ident.clone();
        let default_label = ident.to_string().to_title_case();
        let default_id = format!("{enum_prefix}_{}", ident.to_string().to_snake_case());
        let options = item_options.unwrap_or_default();
        Ok(Self {
            submenus,
            leading,
            spec: VariantSpec {
                ident,
                id: options.id.unwrap_or(default_id),
                label: options.label.unwrap_or(default_label),
                enabled: options.enabled.unwrap_or(true),
                accelerator: options.accelerator,
            },
        })
    }
}

fn parse_submenu_attr(attr: &Attribute) -> syn::Result<SubmenuAttr> {
    attr.parse_args_with(|input: ParseStream| {
        let title: LitStr = input.parse()?;
        if input.is_empty() {
            let segments = title
                .value()
                .split(" > ")
                .map(str::trim)
                .filter(|segment| !segment.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>();
            if segments.is_empty() {
                return Err(syn::Error::new_spanned(
                    title,
                    "submenu title cannot be empty",
                ));
            }
            return Ok(SubmenuAttr::Path(segments));
        }
        input.parse::<Token![,]>()?;
        let key: Ident = input.parse()?;
        if key != "standard" {
            return Err(syn::Error::new_spanned(key, "expected `standard = edit`"));
        }
        input.parse::<Token![=]>()?;
        let preset: Ident = input.parse()?;
        if preset != "edit" {
            return Err(syn::Error::new_spanned(preset, "expected `edit`"));
        }
        if input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
        }
        if !input.is_empty() {
            return Err(input.error("unexpected trailing tokens"));
        }
        Ok(SubmenuAttr::StandardEdit(title.value()))
    })
}

fn parse_predefined_attr(attr: &Attribute) -> syn::Result<&'static str> {
    let ident: Ident = attr.parse_args()?;
    match ident.to_string().as_str() {
        "about" => Ok("About"),
        "undo" => Ok("Undo"),
        "redo" => Ok("Redo"),
        "cut" => Ok("Cut"),
        "copy" => Ok("Copy"),
        "paste" => Ok("Paste"),
        "select_all" => Ok("SelectAll"),
        "minimize" => Ok("Minimize"),
        "maximize" => Ok("Maximize"),
        "fullscreen" => Ok("Fullscreen"),
        "hide" => Ok("Hide"),
        "hide_others" => Ok("HideOthers"),
        "show_all" => Ok("ShowAll"),
        "close_window" => Ok("CloseWindow"),
        "quit" => Ok("Quit"),
        "services" => Ok("Services"),
        "bring_all_to_front" => Ok("BringAllToFront"),
        _ => Err(syn::Error::new_spanned(
            ident,
            "unknown predefined menu item",
        )),
    }
}

#[derive(Default)]
struct ItemOptions {
    label: Option<String>,
    id: Option<String>,
    accelerator: Option<String>,
    enabled: Option<bool>,
}

impl Parse for ItemOptions {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut options = Self::default();
        if input.peek(LitStr) {
            let label: LitStr = input.parse()?;
            options.label = Some(label.value());
            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
        }
        let entries = Punctuated::<ItemOption, Token![,]>::parse_terminated(input)?;
        for entry in entries {
            match entry {
                ItemOption::Id(value) => options.id = Some(value),
                ItemOption::Accelerator(value) => options.accelerator = Some(value),
                ItemOption::Enabled(value) => options.enabled = Some(value),
            }
        }
        Ok(options)
    }
}

enum ItemOption {
    Id(String),
    Accelerator(String),
    Enabled(bool),
}

impl Parse for ItemOption {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let key: Ident = input.parse()?;
        input.parse::<Token![=]>()?;
        if key == "id" {
            Ok(Self::Id(input.parse::<LitStr>()?.value()))
        } else if key == "accelerator" {
            Ok(Self::Accelerator(input.parse::<LitStr>()?.value()))
        } else if key == "enabled" {
            Ok(Self::Enabled(input.parse::<LitBool>()?.value()))
        } else {
            Err(syn::Error::new_spanned(
                key,
                "expected `id`, `accelerator`, or `enabled`",
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_quote;

    #[test]
    fn menu_macro_expands_submenus_items_separators_predefined_and_standard_edit() {
        let item: DeriveInput = parse_quote! {
            #[derive(Clone, Copy, Debug, PartialEq, Eq)]
            enum StudioMenu {
                #[submenu("Studio")]
                #[predefined(about)]
                #[separator]
                #[item("Quit Studio", id = "studio_quit", accelerator = "super+q")]
                Quit,
                #[submenu("File > Recent")]
                #[item("Open...", accelerator = "super+o", enabled = false)]
                Open,
            }
        };
        let declaration =
            Declaration::parse(quote!(standard_edit = "Edit"), item, "::vmux_menu").unwrap();
        let output = declaration.expand().to_string();
        assert!(output.contains("impl :: vmux_menu :: MenuAction for StudioMenu"));
        assert!(output.contains("Self :: Quit => \"studio_quit\""));
        assert!(output.contains("Self :: Open => \"studio_menu_open\""));
        assert!(output.contains("Predefined :: About"));
        assert!(output.contains("Separator"));
        assert!(output.contains("Submenu :: standard_edit (\"Edit\")"));
        assert!(output.contains("with_accelerator (\"super+o\")"));
    }

    #[test]
    fn menu_macro_rejects_variant_without_submenu_and_non_unit_variants() {
        let missing_submenu: DeriveInput = parse_quote! {
            enum BadMenu {
                Open,
            }
        };
        assert!(Declaration::parse(quote!(), missing_submenu, "::vmux_menu").is_err());

        let tuple_variant: DeriveInput = parse_quote! {
            enum BadMenu {
                #[submenu("File")]
                Open(String),
            }
        };
        assert!(Declaration::parse(quote!(), tuple_variant, "::vmux_menu").is_err());
    }
}
