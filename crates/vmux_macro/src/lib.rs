use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{Attribute, Data, DeriveInput, Fields, LitStr, parse_macro_input};

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
        items.push(quote! {
            let #item_ident = ::muda::MenuItem::with_id(#id_lit, #label, true, None);
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
                let mut app_native_submenu = ::muda::Submenu::new("Vmux", true);
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
}

impl MenuProps {
    fn from_attrs(attrs: &[Attribute]) -> syn::Result<Self> {
        let mut id = None;
        let mut label = None;
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
                }
                Ok(())
            })?;
        }
        Ok(MenuProps { id, label })
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
