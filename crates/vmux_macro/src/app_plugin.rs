use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Data, DeriveInput, Fields, Ident, LitStr, Type};

struct PluginInstall {
    option: Ident,
    plugins: Vec<syn::Path>,
    feature: LitStr,
}

struct PluginOption {
    field: Ident,
    feature: LitStr,
    presets: Vec<Ident>,
}

impl PluginOption {
    fn matches(&self, feature: &LitStr, presets: &[Ident]) -> bool {
        self.feature.value() == feature.value()
            && self.presets.len() == presets.len()
            && self
                .presets
                .iter()
                .all(|preset| presets.iter().any(|found| found == preset))
    }
}

pub(crate) fn expand(input: DeriveInput) -> syn::Result<TokenStream> {
    let ident = &input.ident;
    let options = format_ident!("{ident}Options");
    let builder = format_ident!("{ident}Builder");
    let visibility = &input.vis;
    let docs = input
        .attrs
        .iter()
        .filter(|attribute| attribute.path().is_ident("doc"));
    let Data::Enum(data) = &input.data else {
        return Err(syn::Error::new_spanned(
            ident,
            "#[app_plugin] applies to an enum",
        ));
    };
    let mut installs = Vec::new();
    let mut plugin_options = Vec::new();
    let mut preset_names = Vec::new();

    for variant in &data.variants {
        let Fields::Unnamed(unnamed) = &variant.fields else {
            return Err(syn::Error::new_spanned(
                &variant.fields,
                "app plugin variants contain at least one plugin type",
            ));
        };
        if unnamed.unnamed.is_empty() {
            return Err(syn::Error::new_spanned(
                unnamed,
                "app plugin variants contain at least one plugin type",
            ));
        }
        let mut plugins = Vec::new();
        for field in &unnamed.unnamed {
            let Type::Path(plugin) = &field.ty else {
                return Err(syn::Error::new_spanned(
                    &field.ty,
                    "app plugin variants contain plugin paths",
                ));
            };
            plugins.push(plugin.path.clone());
        }
        let mut feature = None;
        let mut option = None;
        let mut presets = Vec::new();
        for attribute in &variant.attrs {
            if !attribute.path().is_ident("plugin") {
                continue;
            }
            attribute.parse_nested_meta(|meta| {
                if meta.path.is_ident("feature") {
                    if feature.is_some() {
                        return Err(meta.error("duplicate feature"));
                    }
                    feature = Some(meta.value()?.parse::<LitStr>()?);
                    return Ok(());
                }
                if meta.path.is_ident("option") {
                    if option.is_some() {
                        return Err(meta.error("duplicate option"));
                    }
                    option = Some(meta.value()?.parse::<Ident>()?);
                    return Ok(());
                }
                let Some(preset) = meta.path.get_ident() else {
                    return Err(meta.error("preset must be an identifier"));
                };
                presets.push(preset.clone());
                Ok(())
            })?;
        }
        let feature = feature.ok_or_else(|| {
            syn::Error::new_spanned(variant, "missing #[plugin(feature = \"...\")]")
        })?;
        if feature.value().is_empty() {
            return Err(syn::Error::new(feature.span(), "feature cannot be empty"));
        }
        let option = option.unwrap_or_else(|| {
            Ident::new(
                &snake_case(&variant.ident.to_string()),
                variant.ident.span(),
            )
        });
        if let Some(existing) = plugin_options
            .iter()
            .find(|entry: &&PluginOption| entry.field == option)
        {
            if !existing.matches(&feature, &presets) {
                return Err(syn::Error::new_spanned(
                    variant,
                    "shared option must use the same feature and presets",
                ));
            }
        } else {
            for preset in &presets {
                if !preset_names
                    .iter()
                    .any(|existing: &Ident| existing == preset)
                {
                    preset_names.push(preset.clone());
                }
            }
            plugin_options.push(PluginOption {
                field: option.clone(),
                feature: feature.clone(),
                presets,
            });
        }
        installs.push(PluginInstall {
            option,
            plugins,
            feature,
        });
    }
    if installs.is_empty() {
        return Err(syn::Error::new_spanned(
            ident,
            "app plugin enum cannot be empty",
        ));
    }

    let option_fields = plugin_options.iter().map(|entry| {
        let field = &entry.field;
        let feature = &entry.feature;
        quote! {
            #[cfg(feature = #feature)]
            #field: bool,
        }
    });
    let none_fields = plugin_options.iter().map(|entry| {
        let field = &entry.field;
        let feature = &entry.feature;
        quote! {
            #[cfg(feature = #feature)]
            #field: false,
        }
    });
    let option_setters = plugin_options.iter().map(|entry| {
        let field = &entry.field;
        let feature = &entry.feature;
        quote! {
            #[cfg(feature = #feature)]
            pub const fn #field(mut self, enabled: bool) -> Self {
                self.#field = enabled;
                self
            }
        }
    });
    let builder_setters = plugin_options.iter().map(|entry| {
        let field = &entry.field;
        let feature = &entry.feature;
        quote! {
            #[cfg(feature = #feature)]
            pub const fn #field(mut self, enabled: bool) -> Self {
                self.options = self.options.#field(enabled);
                self
            }
        }
    });
    let plugin_installs = installs.iter().map(|entry| {
        let field = &entry.option;
        let plugins = &entry.plugins;
        let feature = &entry.feature;
        quote! {
            #[cfg(feature = #feature)]
            if self.options.#field {
                app.add_plugins((#(#plugins,)*));
            }
        }
    });
    let option_presets = preset_names.iter().map(|preset| {
        let preset_feature = LitStr::new(&preset.to_string(), preset.span());
        let values = plugin_options.iter().map(|entry| {
            let field = &entry.field;
            let feature = &entry.feature;
            let enabled = entry.presets.iter().any(|found| found == preset);
            quote! {
                #[cfg(feature = #feature)]
                #field: #enabled,
            }
        });
        quote! {
            #[cfg(feature = #preset_feature)]
            pub const fn #preset() -> Self {
                Self {
                    #(#values)*
                }
            }
        }
    });
    let builder_presets = preset_names.iter().map(|preset| {
        let preset_feature = LitStr::new(&preset.to_string(), preset.span());
        quote! {
            #[cfg(feature = #preset_feature)]
            pub const fn #preset(mut self) -> Self {
                self.options = #options::#preset();
                self
            }
        }
    });

    Ok(quote! {
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        #visibility struct #options {
            #(#option_fields)*
        }

        impl #options {
            pub const fn none() -> Self {
                Self {
                    #(#none_fields)*
                }
            }

            #(#option_presets)*
            #(#option_setters)*
        }

        impl ::core::default::Default for #options {
            fn default() -> Self {
                Self::none()
            }
        }

        #visibility struct #builder {
            options: #options,
        }

        impl #builder {
            #(#builder_presets)*

            pub const fn options(mut self, options: #options) -> Self {
                self.options = options;
                self
            }

            #(#builder_setters)*

            pub const fn build(self) -> #ident {
                #ident {
                    options: self.options,
                }
            }
        }

        impl ::core::default::Default for #builder {
            fn default() -> Self {
                #ident::builder()
            }
        }

        #(#docs)*
        #visibility struct #ident {
            options: #options,
        }

        impl #ident {
            pub const fn builder() -> #builder {
                #builder {
                    options: #options::none(),
                }
            }
        }

        impl ::bevy_app::Plugin for #ident {
            fn build(&self, app: &mut ::bevy_app::App) {
                let _ = (&self.options, &app);
                #(#plugin_installs)*
            }
        }
    })
}

fn snake_case(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    let mut value = String::new();
    for (index, character) in chars.iter().copied().enumerate() {
        let previous = index.checked_sub(1).and_then(|at| chars.get(at)).copied();
        let next = chars.get(index + 1).copied();
        if character.is_uppercase()
            && index > 0
            && (previous.is_some_and(|found| found.is_lowercase() || found.is_numeric())
                || next.is_some_and(char::is_lowercase))
        {
            value.push('_');
        }
        value.extend(character.to_lowercase());
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variant_names_become_builder_methods() {
        assert_eq!(snake_case("Core"), "core");
        assert_eq!(snake_case("MobilePages"), "mobile_pages");
        assert_eq!(snake_case("MCPServer"), "mcp_server");
    }
}
