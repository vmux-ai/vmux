use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{Attribute, Data, DeriveInput, Fields, parse_macro_input};

#[proc_macro_derive(NativeMenuLeaf)]
pub fn derive_native_menu_leaf(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match impl_native_menu_leaf(input) {
        Ok(tokens) => tokens.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

#[proc_macro_derive(NativeMenu)]
pub fn derive_native_menu(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match impl_native_menu(input) {
        Ok(tokens) => tokens.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

fn impl_native_menu_leaf(input: DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let ident = &input.ident;
    let Data::Enum(data) = &input.data else {
        return Err(syn::Error::new_spanned(
            ident,
            "NativeMenuLeaf only supports enums",
        ));
    };

    let mut items = Vec::new();
    let mut item_refs = Vec::new();
    for (idx, variant) in data.variants.iter().enumerate() {
        let Fields::Unit = &variant.fields else {
            return Err(syn::Error::new_spanned(
                &variant.fields,
                "NativeMenuLeaf only supports unit enum variants",
            ));
        };
        let props = StrumProps::from_attrs(&variant.attrs)?;
        let (Some(id), Some(label)) = (&props.id, &props.label) else {
            return Err(syn::Error::new_spanned(
                variant,
                "each variant needs #[strum(props(Id = \"...\", Label = \"...\"))]",
            ));
        };
        let id = id.as_str();
        let label = label.as_str();
        let item_ident = format_ident!("native_menu_item_{}", idx);
        items.push(quote! {
            let #item_ident = ::muda::MenuItem::with_id(#id, #label, true, None);
        });
        item_refs.push(quote! { &#item_ident });
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
        }
    })
}

fn impl_native_menu(input: DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let ident = &input.ident;
    let Data::Enum(data) = &input.data else {
        return Err(syn::Error::new_spanned(
            ident,
            "NativeMenu only supports enums",
        ));
    };

    let mut submenu_blocks = Vec::new();
    let mut submenu_idents = Vec::new();

    for variant in &data.variants {
        let Fields::Unnamed(fields) = &variant.fields else {
            return Err(syn::Error::new_spanned(
                &variant.fields,
                "NativeMenu expects tuple variants like Space(SpaceCommand)",
            ));
        };
        let Some(field) = fields.unnamed.first() else {
            return Err(syn::Error::new_spanned(
                variant,
                "NativeMenu tuple variant needs one field",
            ));
        };
        let inner_ty = &field.ty;
        let props = StrumProps::from_attrs(&variant.attrs)?;
        let Some(title) = props.label.clone() else {
            return Err(syn::Error::new_spanned(
                variant,
                "each variant needs #[strum(props(Label = \"...\"))] for the submenu title",
            ));
        };
        let submenu_ident = syn::Ident::new(
            &format!(
                "{}_native_submenu",
                heck_variant_snake_case(&variant.ident.to_string())
            ),
            variant.ident.span(),
        );
        submenu_idents.push(submenu_ident.clone());
        submenu_blocks.push(quote! {
            let mut #submenu_ident = ::muda::Submenu::new(#title, true);
            <#inner_ty>::append_native_menu_leaf(&mut #submenu_ident)?;
        });
    }

    let submenu_refs: Vec<_> = submenu_idents.iter().map(|i| quote! { &#i }).collect();

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
        }
    })
}

struct StrumProps {
    id: Option<String>,
    label: Option<String>,
}

impl StrumProps {
    fn from_attrs(attrs: &[Attribute]) -> syn::Result<Self> {
        let mut id = None;
        let mut label = None;
        for attr in attrs {
            if !attr.path().is_ident("strum") {
                continue;
            }
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("props") {
                    meta.parse_nested_meta(|inner| {
                        if inner.path.is_ident("Id") {
                            let v: syn::LitStr = inner.value()?.parse()?;
                            id = Some(v.value());
                        } else if inner.path.is_ident("Label") {
                            let v: syn::LitStr = inner.value()?.parse()?;
                            label = Some(v.value());
                        }
                        Ok(())
                    })?;
                }
                Ok(())
            })?;
        }
        Ok(StrumProps { id, label })
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
