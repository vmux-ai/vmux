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
        let (Some(id), Some(label)) = (&props.id, &props.label) else {
            return Err(syn::Error::new_spanned(
                variant,
                "each variant needs #[menu(id = \"...\", label = \"...\")]",
            ));
        };
        let id_lit = id.as_str();
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

        let variant_ident = &variant.ident;
        from_menu_arms.push(quote! {
            #id_lit => ::core::option::Option::Some(#ident::#variant_ident),
        });
    }

    Ok(quote! {
        impl #ident {
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
    let mut submenu_idents = Vec::new();
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
        submenu_idents.push(submenu_ident.clone());
        submenu_blocks.push(quote! {
            let mut #submenu_ident = ::muda::Submenu::new(#title, true);
            <#inner_ty>::append_native_menu_leaf(&mut #submenu_ident)?;
        });

        from_menu_clauses.push(quote! {
            <#inner_ty>::from_menu_id(id).map(#ident::#variant_ident)
        });
    }

    let submenu_refs: Vec<_> = submenu_idents.iter().map(|i| quote! { &#i }).collect();

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
                let app_name = match env!("VMUX_PROFILE") {
                    "release" => "Vmux".to_string(),
                    "local" => format!("Vmux ({})", env!("VMUX_GIT_HASH")),
                    "dev" => "Vmux (Dev)".to_string(),
                    other => format!("Vmux ({})", other),
                };
                let mut app_native_submenu = ::muda::Submenu::new(&app_name, true);
                app_native_submenu.append_items(&[
                    &::muda::PredefinedMenuItem::about(None, None),
                    &::muda::PredefinedMenuItem::separator(),
                    &::muda::PredefinedMenuItem::quit(None),
                ])?;
                #(#submenu_blocks)*
                menu.append_items(&[
                    &app_native_submenu,
                    #(#submenu_refs),*
                ])?;
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
}

impl MenuProps {
    fn from_attrs(attrs: &[Attribute]) -> syn::Result<Self> {
        let mut id = None;
        let mut label = None;
        let mut accel = None;
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
                }
                Ok(())
            })?;
        }
        Ok(MenuProps { id, label, accel })
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

        let binding_str = match (&bind_props.direct, &bind_props.chord) {
            (Some(_), Some(_)) => {
                return Err(syn::Error::new_spanned(
                    variant,
                    "cannot specify both direct and chord on the same variant",
                ));
            }
            (None, None) => continue,
            (Some(s), None) => s.clone(),
            (None, Some(s)) => s.clone(),
        };
        let is_chord = bind_props.chord.is_some();

        let Some(menu_id) = &menu_props.id else {
            return Err(syn::Error::new_spanned(
                variant,
                "variant with #[shortcut(...)] must also have #[menu(id = \"...\")]",
            ));
        };

        let binding_tokens = if is_chord {
            let parts: Vec<&str> = binding_str.split(',').collect();
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
        } else {
            let combo_tokens = parse_key_combo_tokens(&binding_str, variant)?;
            quote! {
                crate::shortcut::Shortcut::Direct(#combo_tokens)
            }
        };

        let menu_id_str = menu_id.as_str();
        binding_entries.push(quote! {
            (#binding_tokens, ::std::string::String::from(#menu_id_str))
        });
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

struct BindProps {
    direct: Option<String>,
    chord: Option<String>,
}

impl BindProps {
    fn from_attrs(attrs: &[Attribute]) -> syn::Result<Self> {
        let mut direct = None;
        let mut chord = None;
        for attr in attrs {
            if !attr.path().is_ident("shortcut") {
                continue;
            }
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("direct") {
                    let v: LitStr = meta.value()?.parse()?;
                    direct = Some(v.value());
                } else if meta.path.is_ident("chord") {
                    let v: LitStr = meta.value()?.parse()?;
                    chord = Some(v.value());
                }
                Ok(())
            })?;
        }
        Ok(BindProps { direct, chord })
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
        let (Some(id), Some(label)) = (&props.id, &props.label) else {
            continue;
        };

        // Split label on \t: name is before, tab-hint is after
        let (name, tab_hint) = if let Some(pos) = label.find('\t') {
            (&label[..pos], Some(label[pos + 1..].to_string()))
        } else {
            (label.as_str(), None)
        };

        // Build display shortcut: prefer accel, fall back to tab hint
        let shortcut = if let Some(ref accel) = props.accel {
            accel_to_display(accel)
        } else if let Some(ref hint) = tab_hint {
            hint.clone()
        } else {
            String::new()
        };

        entries.push(quote! {
            (#id, #name, #shortcut)
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
