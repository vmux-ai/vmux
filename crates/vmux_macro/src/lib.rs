use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{Attribute, Data, DeriveInput, Fields, LitStr, parse_macro_input};

#[proc_macro_derive(CommandBar, attributes(menu))]
pub fn derive_command_bar(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match impl_command_bar(input) {
        Ok(tokens) => tokens.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

#[proc_macro_derive(DefaultShortcuts, attributes(shortcut, menu))]
pub fn derive_default_shortcuts(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match impl_default_shortcuts(input) {
        Ok(tokens) => tokens.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

#[proc_macro_derive(McpTool, attributes(mcp, menu))]
pub fn derive_mcp_tool(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match impl_mcp_tool(input) {
        Ok(tokens) => tokens.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

#[proc_macro_derive(OsSubMenu, attributes(menu))]
pub fn derive_os_sub_menu(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match impl_os_sub_menu(input) {
        Ok(tokens) => tokens.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

#[proc_macro_derive(OsMenu, attributes(menu))]
pub fn derive_os_menu(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match impl_os_menu(input) {
        Ok(tokens) => tokens.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

#[proc_macro_derive(OsSubMenuGroup, attributes(menu))]
pub fn derive_os_sub_menu_group(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match impl_os_sub_menu_group(input) {
        Ok(tokens) => tokens.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

fn impl_os_sub_menu(input: DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let ident = &input.ident;
    let Data::Enum(data) = &input.data else {
        return Err(syn::Error::new_spanned(
            ident,
            "OsSubMenu only supports enums",
        ));
    };

    let mut items = Vec::new();
    let mut item_refs = Vec::new();
    let mut from_menu_arms = Vec::new();

    for (idx, variant) in data.variants.iter().enumerate() {
        let Fields::Unit = &variant.fields else {
            return Err(syn::Error::new_spanned(
                &variant.fields,
                "OsSubMenu only supports unit enum variants",
            ));
        };
        let props = MenuProps::from_attrs(&variant.attrs)?;
        let Some(id) = &props.id else {
            return Err(syn::Error::new_spanned(
                variant,
                "each variant needs #[menu(id = \"...\")]",
            ));
        };
        let id_lit = id.as_str();
        let variant_ident = &variant.ident;
        from_menu_arms.push(quote! {
            #id_lit => ::core::option::Option::Some(#ident::#variant_ident),
        });

        if props.hidden {
            continue;
        }
        let Some(label) = &props.label else {
            return Err(syn::Error::new_spanned(
                variant,
                "visible variants need #[menu(label = \"...\")]",
            ));
        };
        let label = label.as_str();
        let item_ident = format_ident!("os_menu_item_{}", idx);
        let accel_tokens = if let Some(ref accel) = props.accel {
            let accel_str = accel.as_str();
            quote! { Some(#accel_str.parse::<::muda::accelerator::Accelerator>().unwrap()) }
        } else {
            quote! { None }
        };
        items.push(quote! {
            let #item_ident = ::muda::MenuItem::with_id(#id_lit, #label, true, #accel_tokens);
        });
        item_refs.push(quote! { &#item_ident });
    }

    let has_visible = !items.is_empty();

    Ok(quote! {
        impl #ident {
            pub(crate) const HAS_VISIBLE_ITEMS: bool = #has_visible;

            pub(crate) fn append_native_menu_leaf(
                submenu: &mut ::muda::Submenu,
            ) -> Result<(), ::muda::Error> {
                #(#items)*
                submenu.append_items(&[#(#item_refs),*])?;
                Ok(())
            }

            pub fn from_menu_id(id: &str) -> ::core::option::Option<Self> {
                match id {
                    #(#from_menu_arms)*
                    _ => ::core::option::Option::None,
                }
            }
        }
    })
}

fn impl_os_menu(input: DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let ident = &input.ident;
    let Data::Enum(data) = &input.data else {
        return Err(syn::Error::new_spanned(ident, "OsMenu only supports enums"));
    };

    let mut submenu_blocks = Vec::new();
    let mut from_menu_clauses = Vec::new();

    for variant in &data.variants {
        let Fields::Unnamed(fields) = &variant.fields else {
            return Err(syn::Error::new_spanned(
                &variant.fields,
                "OsMenu expects tuple variants like Space(SpaceCommand)",
            ));
        };
        let Some(field) = fields.unnamed.first() else {
            return Err(syn::Error::new_spanned(
                variant,
                "OsMenu tuple variant needs one field",
            ));
        };
        let inner_ty = &field.ty;
        let props = MenuProps::from_attrs(&variant.attrs)?;
        let Some(title) = props.label.clone() else {
            return Err(syn::Error::new_spanned(
                variant,
                "each variant needs #[menu(label = \"...\")] for the submenu title",
            ));
        };
        let variant_ident = &variant.ident;
        let submenu_ident = syn::Ident::new(
            &format!(
                "{}_os_submenu",
                heck_variant_snake_case(&variant.ident.to_string())
            ),
            variant.ident.span(),
        );
        submenu_blocks.push(quote! {
            if <#inner_ty>::HAS_VISIBLE_ITEMS {
                let mut #submenu_ident = ::muda::Submenu::new(#title, true);
                <#inner_ty>::append_native_menu_leaf(&mut #submenu_ident)?;
                submenus.push(::std::boxed::Box::new(#submenu_ident) as ::std::boxed::Box<dyn ::muda::IsMenuItem>);
            }
        });

        from_menu_clauses.push(quote! {
            <#inner_ty>::from_menu_id(id).map(#ident::#variant_ident)
        });
    }

    let from_menu_body = if from_menu_clauses.is_empty() {
        quote! { ::core::option::Option::None }
    } else {
        let first = &from_menu_clauses[0];
        let chained = from_menu_clauses[1..]
            .iter()
            .fold(quote! { #first }, |acc, c| quote! { #acc.or_else(|| #c) });
        quote! { #chained }
    };

    Ok(quote! {
        impl #ident {
            pub(crate) fn build_native_root_menu(menu: &mut ::muda::Menu) -> Result<(), ::muda::Error> {
                let app_name = match env!("VMUX_BUILD_PROFILE") {
                    "release" => "Vmux".to_string(),
                    "local" => format!("Vmux ({})", env!("VMUX_GIT_HASH")),
                    "dev" => format!("Vmux Dev ({})", env!("VMUX_GIT_HASH")),
                    other => format!("Vmux ({})", other),
                };
                let mut app_native_submenu = ::muda::Submenu::new(&app_name, true);
                let quit_label = format!("Quit {}", &app_name);
                let quit_item = ::muda::MenuItem::with_id(
                    "app_quit",
                    &quit_label,
                    true,
                    ::core::option::Option::Some("super+q".parse().unwrap()),
                );
                app_native_submenu.append_items(&[
                    &::muda::PredefinedMenuItem::about(None, None),
                    &::muda::PredefinedMenuItem::separator(),
                    &quit_item,
                ])?;
                let mut submenus: ::std::vec::Vec<::std::boxed::Box<dyn ::muda::IsMenuItem>> = ::std::vec::Vec::new();
                submenus.push(::std::boxed::Box::new(app_native_submenu));
                #(#submenu_blocks)*
                let refs: ::std::vec::Vec<&dyn ::muda::IsMenuItem> = submenus.iter().map(|s| s.as_ref()).collect();
                menu.append_items(&refs)?;
                Ok(())
            }

            pub fn from_menu_id(id: &str) -> ::core::option::Option<Self> {
                #from_menu_body
            }
        }
    })
}

fn impl_os_sub_menu_group(input: DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let ident = &input.ident;
    let Data::Enum(data) = &input.data else {
        return Err(syn::Error::new_spanned(
            ident,
            "OsSubMenuGroup only supports enums",
        ));
    };

    let mut nested_blocks = Vec::new();
    let mut visible_terms = Vec::new();
    let mut from_menu_clauses = Vec::new();

    for variant in &data.variants {
        let Fields::Unnamed(fields) = &variant.fields else {
            return Err(syn::Error::new_spanned(
                &variant.fields,
                "OsSubMenuGroup expects tuple variants like Window(WindowCommand)",
            ));
        };
        let Some(field) = fields.unnamed.first() else {
            return Err(syn::Error::new_spanned(
                variant,
                "OsSubMenuGroup tuple variant needs one field",
            ));
        };
        let inner_ty = &field.ty;
        let props = MenuProps::from_attrs(&variant.attrs)?;
        let Some(title) = props.label.clone() else {
            return Err(syn::Error::new_spanned(
                variant,
                "each variant needs #[menu(label = \"...\")] for the nested submenu title",
            ));
        };
        let variant_ident = &variant.ident;
        let nested_ident = syn::Ident::new(
            &format!(
                "{}_nested_submenu",
                heck_variant_snake_case(&variant.ident.to_string())
            ),
            variant.ident.span(),
        );

        nested_blocks.push(quote! {
            if <#inner_ty>::HAS_VISIBLE_ITEMS {
                let mut #nested_ident = ::muda::Submenu::new(#title, true);
                <#inner_ty>::append_native_menu_leaf(&mut #nested_ident)?;
                submenu.append(&#nested_ident)?;
            }
        });

        visible_terms.push(quote! { <#inner_ty>::HAS_VISIBLE_ITEMS });
        from_menu_clauses.push(quote! {
            <#inner_ty>::from_menu_id(id).map(#ident::#variant_ident)
        });
    }

    let visible_expr = if visible_terms.is_empty() {
        quote! { false }
    } else {
        let first = &visible_terms[0];
        visible_terms[1..]
            .iter()
            .fold(quote! { #first }, |acc, t| quote! { #acc || #t })
    };

    let from_menu_body = if from_menu_clauses.is_empty() {
        quote! { ::core::option::Option::None }
    } else {
        let first = &from_menu_clauses[0];
        from_menu_clauses[1..]
            .iter()
            .fold(quote! { #first }, |acc, c| quote! { #acc.or_else(|| #c) })
    };

    Ok(quote! {
        impl #ident {
            pub(crate) const HAS_VISIBLE_ITEMS: bool = #visible_expr;

            pub(crate) fn append_native_menu_leaf(
                submenu: &mut ::muda::Submenu,
            ) -> Result<(), ::muda::Error> {
                #(#nested_blocks)*
                Ok(())
            }

            pub fn from_menu_id(id: &str) -> ::core::option::Option<Self> {
                #from_menu_body
            }
        }
    })
}

struct MenuProps {
    id: Option<String>,
    label: Option<String>,
    accel: Option<String>,
    hidden: bool,
}

impl MenuProps {
    fn from_attrs(attrs: &[Attribute]) -> syn::Result<Self> {
        let mut id = None;
        let mut label = None;
        let mut accel = None;
        let mut hidden = false;
        for attr in attrs {
            if !attr.path().is_ident("menu") {
                continue;
            }
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("id") {
                    let v: LitStr = meta.value()?.parse()?;
                    id = Some(v.value());
                } else if meta.path.is_ident("label") {
                    let v: LitStr = meta.value()?.parse()?;
                    label = Some(v.value());
                } else if meta.path.is_ident("accel") {
                    let v: LitStr = meta.value()?.parse()?;
                    accel = Some(v.value());
                } else if meta.path.is_ident("hidden") {
                    hidden = true;
                }
                Ok(())
            })?;
        }
        Ok(MenuProps {
            id,
            label,
            accel,
            hidden,
        })
    }
}

fn heck_variant_snake_case(s: &str) -> String {
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if ch.is_uppercase() && i > 0 {
            out.push('_');
        }
        out.push(ch.to_ascii_lowercase());
    }
    out
}

fn impl_default_shortcuts(input: DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let ident = &input.ident;
    let Data::Enum(data) = &input.data else {
        return Err(syn::Error::new_spanned(
            ident,
            "DefaultShortcuts only supports enums",
        ));
    };

    let first_variant = data.variants.first();
    let is_leaf = first_variant
        .map(|v| matches!(v.fields, Fields::Unit))
        .unwrap_or(true);

    if is_leaf {
        impl_leaf_shortcuts(ident, data)
    } else {
        impl_root_shortcuts(ident, data)
    }
}

fn impl_leaf_shortcuts(
    ident: &syn::Ident,
    data: &syn::DataEnum,
) -> syn::Result<proc_macro2::TokenStream> {
    let mut binding_entries = Vec::new();

    for variant in &data.variants {
        let bind_props = BindProps::from_attrs(&variant.attrs)?;
        let menu_props = MenuProps::from_attrs(&variant.attrs)?;

        if bind_props.bindings.is_empty() {
            continue;
        }

        let Some(menu_id) = &menu_props.id else {
            return Err(syn::Error::new_spanned(
                variant,
                "variant with #[shortcut(...)] must also have #[menu(id = \"...\")]",
            ));
        };
        let menu_id_str = menu_id.as_str();

        for binding in &bind_props.bindings {
            let binding_tokens = match binding {
                Binding::Chord(s) => {
                    let parts: Vec<&str> = s.split(',').collect();
                    if parts.len() != 2 {
                        return Err(syn::Error::new_spanned(
                            variant,
                            "chord binding must have exactly two parts separated by comma",
                        ));
                    }
                    let prefix_tokens = parse_key_combo_tokens(parts[0].trim(), variant)?;
                    let second_tokens = parse_key_combo_tokens(parts[1].trim(), variant)?;
                    quote! {
                        crate::shortcut::Shortcut::Chord(#prefix_tokens, #second_tokens)
                    }
                }
                Binding::Direct(s) => {
                    let combo_tokens = parse_key_combo_tokens(s, variant)?;
                    quote! {
                        crate::shortcut::Shortcut::Direct(#combo_tokens)
                    }
                }
            };

            binding_entries.push(quote! {
                (#binding_tokens, ::std::string::String::from(#menu_id_str))
            });
        }
    }

    Ok(quote! {
        impl #ident {
            pub fn default_shortcuts() -> ::std::vec::Vec<(crate::shortcut::Shortcut, ::std::string::String)> {
                ::std::vec![#(#binding_entries),*]
            }
        }
    })
}

fn impl_root_shortcuts(
    ident: &syn::Ident,
    data: &syn::DataEnum,
) -> syn::Result<proc_macro2::TokenStream> {
    let mut extend_calls = Vec::new();

    for variant in &data.variants {
        let Fields::Unnamed(fields) = &variant.fields else {
            return Err(syn::Error::new_spanned(
                &variant.fields,
                "DefaultShortcuts root expects tuple variants",
            ));
        };
        let Some(field) = fields.unnamed.first() else {
            return Err(syn::Error::new_spanned(
                variant,
                "tuple variant needs one field",
            ));
        };
        let inner_ty = &field.ty;
        extend_calls.push(quote! {
            bindings.extend(<#inner_ty>::default_shortcuts());
        });
    }

    Ok(quote! {
        impl #ident {
            pub fn default_shortcuts() -> ::std::vec::Vec<(crate::shortcut::Shortcut, ::std::string::String)> {
                let mut bindings = ::std::vec::Vec::new();
                #(#extend_calls)*
                bindings
            }
        }
    })
}

struct ResolvedKey {
    key_code: String,
    implicit_shift: bool,
}

fn resolve_char_literal(c: char) -> Option<ResolvedKey> {
    let (key_code, shifted) = match c {
        'a'..='z' => (format!("Key{}", c.to_ascii_uppercase()), false),
        'A'..='Z' => (format!("Key{}", c), true),
        '0'..='9' => (format!("Digit{}", c), false),
        ')' => ("Digit0".into(), true),
        '!' => ("Digit1".into(), true),
        '@' => ("Digit2".into(), true),
        '#' => ("Digit3".into(), true),
        '$' => ("Digit4".into(), true),
        '%' => ("Digit5".into(), true),
        '^' => ("Digit6".into(), true),
        '&' => ("Digit7".into(), true),
        '*' => ("Digit8".into(), true),
        '(' => ("Digit9".into(), true),
        '-' => ("Minus".into(), false),
        '_' => ("Minus".into(), true),
        '=' => ("Equal".into(), false),
        '/' => ("Slash".into(), false),
        '?' => ("Slash".into(), true),
        '.' => ("Period".into(), false),
        '>' => ("Period".into(), true),
        ',' => ("Comma".into(), false),
        '<' => ("Comma".into(), true),
        ';' => ("Semicolon".into(), false),
        ':' => ("Semicolon".into(), true),
        '\'' => ("Quote".into(), false),
        '"' => ("Quote".into(), true),
        '[' => ("BracketLeft".into(), false),
        '{' => ("BracketLeft".into(), true),
        ']' => ("BracketRight".into(), false),
        '}' => ("BracketRight".into(), true),
        '\\' => ("Backslash".into(), false),
        '|' => ("Backslash".into(), true),
        '`' => ("Backquote".into(), false),
        '~' => ("Backquote".into(), true),
        ' ' => ("Space".into(), false),
        _ => return None,
    };
    Some(ResolvedKey {
        key_code,
        implicit_shift: shifted,
    })
}

fn parse_key_combo_tokens(
    s: &str,
    spanned: &syn::Variant,
) -> syn::Result<proc_macro2::TokenStream> {
    let parts: Vec<&str> = s.split('+').map(|p| p.trim()).collect();
    let mut ctrl = false;
    let mut shift = false;
    let mut alt = false;
    let mut super_key = false;
    let mut key_name: Option<String> = None;
    let mut implicit_shift = false;

    for part in &parts {
        match *part {
            "Ctrl" => ctrl = true,
            "Shift" => shift = true,
            "Alt" => alt = true,
            "Super" => super_key = true,
            other => {
                if key_name.is_some() {
                    return Err(syn::Error::new_spanned(
                        spanned,
                        format!("multiple non-modifier keys in binding: {s}"),
                    ));
                }
                let chars: Vec<char> = other.chars().collect();
                if chars.len() == 1 {
                    if let Some(resolved) = resolve_char_literal(chars[0]) {
                        key_name = Some(resolved.key_code);
                        implicit_shift = resolved.implicit_shift;
                    } else {
                        return Err(syn::Error::new_spanned(
                            spanned,
                            format!("unrecognized character literal '{}'", chars[0]),
                        ));
                    }
                } else {
                    key_name = Some(other.to_string());
                }
            }
        }
    }

    let Some(key_str) = key_name else {
        return Err(syn::Error::new_spanned(
            spanned,
            format!("no key specified in binding: {s}"),
        ));
    };

    shift = shift || implicit_shift;
    let key_ident = format_ident!("{}", key_str);

    Ok(quote! {
        crate::shortcut::KeyCombo {
            key: ::bevy::input::keyboard::KeyCode::#key_ident,
            modifiers: crate::shortcut::Modifiers {
                ctrl: #ctrl,
                shift: #shift,
                alt: #alt,
                super_key: #super_key,
            },
        }
    })
}

enum Binding {
    Direct(String),
    Chord(String),
}

struct BindProps {
    bindings: Vec<Binding>,
}

impl BindProps {
    fn from_attrs(attrs: &[Attribute]) -> syn::Result<Self> {
        let mut bindings = Vec::new();
        for attr in attrs {
            if !attr.path().is_ident("shortcut") {
                continue;
            }
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("direct") {
                    let v: LitStr = meta.value()?.parse()?;
                    bindings.push(Binding::Direct(v.value()));
                } else if meta.path.is_ident("chord") {
                    let v: LitStr = meta.value()?.parse()?;
                    bindings.push(Binding::Chord(v.value()));
                }
                Ok(())
            })?;
        }
        Ok(BindProps { bindings })
    }
}

// ---------------------------------------------------------------------------
// CommandBar derive
// ---------------------------------------------------------------------------

fn impl_command_bar(input: DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let ident = &input.ident;
    let Data::Enum(data) = &input.data else {
        return Err(syn::Error::new_spanned(
            ident,
            "CommandBar only supports enums",
        ));
    };

    let first_variant = data.variants.first();
    let is_leaf = first_variant
        .map(|v| matches!(v.fields, Fields::Unit))
        .unwrap_or(true);

    if is_leaf {
        impl_command_bar_leaf(ident, data)
    } else {
        impl_command_bar_root(ident, data)
    }
}

fn impl_command_bar_leaf(
    ident: &syn::Ident,
    data: &syn::DataEnum,
) -> syn::Result<proc_macro2::TokenStream> {
    let mut entries = Vec::new();

    for variant in &data.variants {
        let props = MenuProps::from_attrs(&variant.attrs)?;

        let Some(id) = &props.id else {
            continue;
        };
        let id_lit = id.as_str();

        if props.hidden {
            continue;
        }
        let Some(label) = &props.label else {
            continue;
        };

        let (name, tab_hint) = if let Some(pos) = label.find('\t') {
            (&label[..pos], Some(label[pos + 1..].to_string()))
        } else {
            (label.as_str(), None)
        };

        let shortcut = if let Some(ref accel) = props.accel {
            accel_to_display(accel)
        } else if let Some(ref hint) = tab_hint {
            hint.clone()
        } else {
            String::new()
        };

        entries.push(quote! {
            (#id_lit, #name, #shortcut)
        });
    }

    Ok(quote! {
        impl #ident {
            pub fn command_bar_entries() -> ::std::vec::Vec<(&'static str, &'static str, &'static str)> {
                ::std::vec![#(#entries),*]
            }
        }
    })
}

fn impl_command_bar_root(
    ident: &syn::Ident,
    data: &syn::DataEnum,
) -> syn::Result<proc_macro2::TokenStream> {
    let mut extend_calls = Vec::new();

    for variant in &data.variants {
        let Fields::Unnamed(fields) = &variant.fields else {
            return Err(syn::Error::new_spanned(
                &variant.fields,
                "CommandBar root expects tuple variants",
            ));
        };
        let Some(field) = fields.unnamed.first() else {
            return Err(syn::Error::new_spanned(
                variant,
                "tuple variant needs one field",
            ));
        };
        let inner_ty = &field.ty;
        extend_calls.push(quote! {
            entries.extend(<#inner_ty>::command_bar_entries());
        });
    }

    Ok(quote! {
        impl #ident {
            pub fn command_bar_entries() -> ::std::vec::Vec<(&'static str, &'static str, &'static str)> {
                let mut entries = ::std::vec::Vec::new();
                #(#extend_calls)*
                entries
            }
        }
    })
}

struct McpProps {
    description: Option<String>,
    skip: bool,
}

impl McpProps {
    fn from_attrs(attrs: &[Attribute]) -> syn::Result<Self> {
        let mut description = None;
        let mut skip = false;
        for attr in attrs {
            if !attr.path().is_ident("mcp") {
                continue;
            }
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("description") {
                    let v: LitStr = meta.value()?.parse()?;
                    description = Some(v.value());
                } else if meta.path.is_ident("skip") {
                    skip = true;
                } else if meta.path.is_ident("enum_values") {
                    let _ = meta.value()?;
                }
                Ok(())
            })?;
        }
        Ok(McpProps { description, skip })
    }
}

struct McpFieldProps {
    enum_values: Vec<String>,
}

impl McpFieldProps {
    fn from_attrs(attrs: &[Attribute]) -> syn::Result<Self> {
        let mut enum_values = Vec::new();
        for attr in attrs {
            if !attr.path().is_ident("mcp") {
                continue;
            }
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("enum_values") {
                    let value: syn::ExprArray = meta.value()?.parse()?;
                    for el in value.elems {
                        if let syn::Expr::Lit(syn::ExprLit {
                            lit: syn::Lit::Str(s),
                            ..
                        }) = el
                        {
                            enum_values.push(s.value());
                        } else {
                            return Err(meta.error("enum_values must contain string literals"));
                        }
                    }
                }
                Ok(())
            })?;
        }
        Ok(McpFieldProps { enum_values })
    }
}

fn unwrap_option(ty: &syn::Type) -> Option<&syn::Type> {
    let syn::Type::Path(path) = ty else {
        return None;
    };
    let last = path.path.segments.last()?;
    if last.ident != "Option" {
        return None;
    }
    let syn::PathArguments::AngleBracketed(args) = &last.arguments else {
        return None;
    };
    let syn::GenericArgument::Type(inner) = args.args.first()? else {
        return None;
    };
    Some(inner)
}

fn type_schema_kind(ty: &syn::Type) -> Option<&'static str> {
    let syn::Type::Path(path) = ty else {
        return None;
    };
    let last = path.path.segments.last()?;
    let name = last.ident.to_string();
    Some(match name.as_str() {
        "String" => "string",
        "u8" | "u16" | "u32" | "u64" | "i8" | "i16" | "i32" | "i64" => "integer",
        "bool" => "boolean",
        "Value" => "json",
        _ => return None,
    })
}

fn impl_mcp_tool(input: DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let ident = &input.ident;
    let Data::Enum(data) = &input.data else {
        return Err(syn::Error::new_spanned(
            ident,
            "McpTool only supports enums",
        ));
    };

    let first_variant = data.variants.first();
    let is_root_tuple = first_variant
        .map(|v| matches!(v.fields, Fields::Unnamed(_)))
        .unwrap_or(false);

    if is_root_tuple {
        return impl_mcp_tool_root(ident, data);
    }

    let any_fielded = data
        .variants
        .iter()
        .any(|v| matches!(v.fields, Fields::Named(_)));

    if any_fielded {
        impl_mcp_tool_leaf_fielded(ident, data)
    } else {
        impl_mcp_tool_leaf_unit(ident, data)
    }
}

fn impl_mcp_tool_leaf_fielded(
    ident: &syn::Ident,
    data: &syn::DataEnum,
) -> syn::Result<proc_macro2::TokenStream> {
    let mut entries = Vec::new();
    let mut call_arms = Vec::new();

    for variant in &data.variants {
        let mcp_props = McpProps::from_attrs(&variant.attrs)?;
        if mcp_props.skip {
            continue;
        }
        let variant_ident = &variant.ident;
        let tool_name = heck_variant_snake_case(&variant_ident.to_string());
        let description = mcp_props.description.clone().ok_or_else(|| {
            syn::Error::new_spanned(
                variant_ident,
                "fielded McpTool variants require #[mcp(description = \"...\")]",
            )
        })?;

        let mut property_inserts = Vec::new();
        let mut required_strs = Vec::new();
        let mut field_extracts = Vec::new();
        let mut field_constructs = Vec::new();

        let Fields::Named(fields) = &variant.fields else {
            return Err(syn::Error::new_spanned(
                variant,
                "expected named-field variant",
            ));
        };

        for field in &fields.named {
            let field_ident = field.ident.as_ref().expect("named field has ident");
            let field_name = field_ident.to_string();
            let field_props = McpFieldProps::from_attrs(&field.attrs)?;

            let (effective_ty, is_optional) = if let Some(inner) = unwrap_option(&field.ty) {
                (inner.clone(), true)
            } else {
                (field.ty.clone(), false)
            };

            let kind = type_schema_kind(&effective_ty).ok_or_else(|| {
                syn::Error::new_spanned(
                    &field.ty,
                    "unsupported McpTool field type (use String, integer types, bool, or Option<T>)",
                )
            })?;

            let schema_fragment = if !field_props.enum_values.is_empty() {
                if kind != "string" {
                    return Err(syn::Error::new_spanned(
                        &field.ty,
                        "#[mcp(enum_values = ...)] requires String/Option<String>",
                    ));
                }
                let values = &field_props.enum_values;
                quote! {
                    ::serde_json::json!({"type": "string", "enum": [ #(#values),* ]})
                }
            } else if kind == "json" {
                quote! {
                    ::serde_json::json!({})
                }
            } else {
                quote! {
                    ::serde_json::json!({"type": #kind})
                }
            };

            property_inserts.push(quote! {
                properties.insert(#field_name.to_string(), #schema_fragment);
            });
            if !is_optional {
                required_strs.push(field_name.clone());
            }

            let extract = match kind {
                "string" => {
                    let extract = quote! {
                        args.get(#field_name).and_then(|v| v.as_str()).map(::std::string::String::from)
                    };
                    if is_optional {
                        quote! { let #field_ident: ::core::option::Option<::std::string::String> = #extract; }
                    } else {
                        quote! {
                            let #field_ident: ::std::string::String = match #extract {
                                ::core::option::Option::Some(v) => v,
                                ::core::option::Option::None => {
                                    return ::core::option::Option::Some(
                                        ::core::result::Result::Err(format!("{} is required", #field_name))
                                    );
                                }
                            };
                        }
                    }
                }
                "integer" => {
                    let ty_ident = match &effective_ty {
                        syn::Type::Path(p) => &p.path.segments.last().unwrap().ident,
                        _ => unreachable!(),
                    };
                    let extract = quote! {
                        args.get(#field_name)
                            .and_then(|v| v.as_i64())
                            .and_then(|n| <#ty_ident as ::core::convert::TryFrom<i64>>::try_from(n).ok())
                    };
                    if is_optional {
                        quote! { let #field_ident: ::core::option::Option<#ty_ident> = #extract; }
                    } else {
                        quote! {
                            let #field_ident: #ty_ident = match #extract {
                                ::core::option::Option::Some(v) => v,
                                ::core::option::Option::None => {
                                    return ::core::option::Option::Some(
                                        ::core::result::Result::Err(format!("{} is required (integer)", #field_name))
                                    );
                                }
                            };
                        }
                    }
                }
                "boolean" => {
                    let extract = quote! {
                        args.get(#field_name).and_then(|v| v.as_bool())
                    };
                    if is_optional {
                        quote! { let #field_ident: ::core::option::Option<bool> = #extract; }
                    } else {
                        quote! {
                            let #field_ident: bool = match #extract {
                                ::core::option::Option::Some(v) => v,
                                ::core::option::Option::None => {
                                    return ::core::option::Option::Some(
                                        ::core::result::Result::Err(format!("{} is required (boolean)", #field_name))
                                    );
                                }
                            };
                        }
                    }
                }
                "json" => {
                    let extract = quote! {
                        args.get(#field_name).cloned()
                    };
                    if is_optional {
                        quote! { let #field_ident: ::core::option::Option<::serde_json::Value> = #extract; }
                    } else {
                        quote! {
                            let #field_ident: ::serde_json::Value = match #extract {
                                ::core::option::Option::Some(v) => v,
                                ::core::option::Option::None => {
                                    return ::core::option::Option::Some(
                                        ::core::result::Result::Err(format!("{} is required", #field_name))
                                    );
                                }
                            };
                        }
                    }
                }
                _ => unreachable!(),
            };

            field_extracts.push(extract);
            field_constructs.push(quote! { #field_ident });
        }

        let required_array = if required_strs.is_empty() {
            quote! { ::serde_json::json!([]) }
        } else {
            let strs = &required_strs;
            quote! { ::serde_json::json!([ #(#strs),* ]) }
        };

        entries.push(quote! {
            ({
                let mut properties = ::serde_json::Map::new();
                #(#property_inserts)*
                let schema = ::serde_json::json!({
                    "type": "object",
                    "properties": ::serde_json::Value::Object(properties),
                    "required": #required_array
                });
                (#tool_name, #description, schema)
            })
        });

        call_arms.push(quote! {
            #tool_name => {
                #(#field_extracts)*
                ::core::option::Option::Some(::core::result::Result::Ok(
                    #ident::#variant_ident { #(#field_constructs),* }
                ))
            }
        });
    }

    Ok(quote! {
        impl #ident {
            pub fn mcp_tool_entries() -> ::std::vec::Vec<(&'static str, &'static str, ::serde_json::Value)> {
                ::std::vec![#(#entries),*]
            }

            pub fn from_mcp_call(
                name: &str,
                args: ::serde_json::Value,
            ) -> ::core::option::Option<::core::result::Result<Self, ::std::string::String>> {
                match name {
                    #(#call_arms)*
                    _ => ::core::option::Option::None,
                }
            }
        }
    })
}

fn impl_mcp_tool_leaf_unit(
    ident: &syn::Ident,
    data: &syn::DataEnum,
) -> syn::Result<proc_macro2::TokenStream> {
    let mut entries = Vec::new();
    let mut id_arms = Vec::new();

    for variant in &data.variants {
        let mcp_props = McpProps::from_attrs(&variant.attrs)?;
        if mcp_props.skip {
            continue;
        }
        let menu_props = MenuProps::from_attrs(&variant.attrs)?;
        let id = match &menu_props.id {
            Some(id) => id.clone(),
            None if mcp_props.description.is_some() => {
                heck_variant_snake_case(&variant.ident.to_string())
            }
            None => continue,
        };
        let id_lit = id.as_str();
        let variant_ident = &variant.ident;

        let description = mcp_props
            .description
            .clone()
            .or_else(|| {
                menu_props
                    .label
                    .as_deref()
                    .map(|l| l.split('\t').next().unwrap_or(l).trim().to_string())
            })
            .unwrap_or_default();

        entries.push(quote! {
            (#id_lit, #description, ::serde_json::json!({
                "type": "object",
                "properties": {}
            }))
        });
        id_arms.push(quote! {
            #id_lit => ::core::option::Option::Some(#ident::#variant_ident),
        });
    }

    Ok(quote! {
        impl #ident {
            pub fn mcp_tool_entries() -> ::std::vec::Vec<(&'static str, &'static str, ::serde_json::Value)> {
                ::std::vec![#(#entries),*]
            }

            pub fn from_mcp_id(id: &str) -> ::core::option::Option<Self> {
                match id {
                    #(#id_arms)*
                    _ => ::core::option::Option::None,
                }
            }
        }
    })
}

fn impl_mcp_tool_root(
    ident: &syn::Ident,
    data: &syn::DataEnum,
) -> syn::Result<proc_macro2::TokenStream> {
    let mut extend_calls = Vec::new();
    let mut id_clauses = Vec::new();

    for variant in &data.variants {
        let Fields::Unnamed(fields) = &variant.fields else {
            return Err(syn::Error::new_spanned(
                &variant.fields,
                "McpTool root expects tuple variants",
            ));
        };
        let Some(field) = fields.unnamed.first() else {
            return Err(syn::Error::new_spanned(
                variant,
                "tuple variant needs one field",
            ));
        };
        let inner_ty = &field.ty;
        let variant_ident = &variant.ident;
        extend_calls.push(quote! {
            entries.extend(<#inner_ty>::mcp_tool_entries());
        });
        id_clauses.push(quote! {
            <#inner_ty>::from_mcp_id(id).map(#ident::#variant_ident)
        });
    }

    let from_id_body = if id_clauses.is_empty() {
        quote! { ::core::option::Option::None }
    } else {
        let first = &id_clauses[0];
        let chained = id_clauses[1..]
            .iter()
            .fold(quote! { #first }, |acc, c| quote! { #acc.or_else(|| #c) });
        quote! { #chained }
    };

    Ok(quote! {
        impl #ident {
            pub fn mcp_tool_entries() -> ::std::vec::Vec<(&'static str, &'static str, ::serde_json::Value)> {
                let mut entries = ::std::vec::Vec::new();
                #(#extend_calls)*
                entries
            }

            pub fn from_mcp_id(id: &str) -> ::core::option::Option<Self> {
                #from_id_body
            }
        }
    })
}

/// Convert muda accelerator format to display symbols.
/// e.g. "super+shift+r" → "⌘⇧R", "super+alt+i" → "⌘⌥I"
fn accel_to_display(accel: &str) -> String {
    let parts: Vec<&str> = accel.split('+').map(|p| p.trim()).collect();
    let mut out = String::new();
    let mut key = "";

    for part in &parts {
        match *part {
            "super" => out.push('\u{2318}'), // ⌘
            "shift" => out.push('\u{21e7}'), // ⇧
            "alt" => out.push('\u{2325}'),   // ⌥
            "ctrl" => out.push('^'),
            other => key = other,
        }
    }

    // Capitalise the key for display
    match key {
        "tab" => out.push('\u{21e5}'),   // ⇥
        "space" => out.push('\u{2423}'), // ␣
        "enter" => out.push('\u{21a9}'), // ↩
        "escape" => out.push_str("Esc"),
        "delete" => out.push('\u{232b}'), // ⌫
        "[" => out.push('['),
        "]" => out.push(']'),
        "=" => out.push('='),
        "-" => out.push('-'),
        "," => out.push(','),
        "." => out.push('.'),
        "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" => out.push_str(key),
        _ => {
            for ch in key.chars() {
                out.push(ch.to_ascii_uppercase());
            }
        }
    }

    out
}
