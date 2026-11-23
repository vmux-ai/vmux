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
    requires: Vec<Ident>,
}

impl PluginOption {
    fn matches(&self, feature: &LitStr, presets: &[Ident], requires: &[Ident]) -> bool {
        self.feature.value() == feature.value()
            && self.presets.len() == presets.len()
            && self
                .presets
                .iter()
                .all(|preset| presets.iter().any(|found| found == preset))
            && self.requires.len() == requires.len()
            && self
                .requires
                .iter()
                .all(|required| requires.iter().any(|found| found == required))
    }
}

struct AppPlugin {
    ident: Ident,
    visibility: syn::Visibility,
    docs: Vec<syn::Attribute>,
    installs: Vec<PluginInstall>,
    plugin_options: Vec<PluginOption>,
    preset_names: Vec<Ident>,
}

impl AppPlugin {
    fn parse(input: DeriveInput) -> syn::Result<Self> {
        let ident = input.ident.clone();
        let visibility = input.vis.clone();
        let docs = input
            .attrs
            .iter()
            .filter(|attribute| attribute.path().is_ident("doc"))
            .cloned()
            .collect();
        let Data::Enum(data) = &input.data else {
            return Err(syn::Error::new_spanned(
                &ident,
                "#[app_plugin] applies to an enum",
            ));
        };
        let mut parsed = Self {
            ident,
            visibility,
            docs,
            installs: Vec::new(),
            plugin_options: Vec::new(),
            preset_names: Vec::new(),
        };
        for variant in &data.variants {
            parsed.parse_variant(variant)?;
        }
        if parsed.installs.is_empty() {
            return Err(syn::Error::new_spanned(
                &parsed.ident,
                "app plugin enum cannot be empty",
            ));
        }
        for option in &parsed.plugin_options {
            dependency_closure(option, &parsed.plugin_options)?;
        }
        Ok(parsed)
    }

    fn parse_variant(&mut self, variant: &syn::Variant) -> syn::Result<()> {
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
        let (feature, option, presets, requires) = Self::parse_attributes(variant)?;
        if let Some(existing) = self
            .plugin_options
            .iter()
            .find(|entry| entry.field == option)
        {
            if !existing.matches(&feature, &presets, &requires) {
                return Err(syn::Error::new_spanned(
                    variant,
                    "shared option must use the same feature, presets, and dependencies",
                ));
            }
        } else {
            for preset in &presets {
                if !self.preset_names.iter().any(|existing| existing == preset) {
                    self.preset_names.push(preset.clone());
                }
            }
            self.plugin_options.push(PluginOption {
                field: option.clone(),
                feature: feature.clone(),
                presets,
                requires,
            });
        }
        self.installs.push(PluginInstall {
            option,
            plugins,
            feature,
        });
        Ok(())
    }

    fn parse_attributes(
        variant: &syn::Variant,
    ) -> syn::Result<(LitStr, Ident, Vec<Ident>, Vec<Ident>)> {
        let mut feature = None;
        let mut option = None;
        let mut presets = Vec::new();
        let mut requires = Vec::new();
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
                if meta.path.is_ident("requires") {
                    meta.parse_nested_meta(|dependency| {
                        let Some(required) = dependency.path.get_ident() else {
                            return Err(dependency.error("dependency must be an identifier"));
                        };
                        requires.push(required.clone());
                        Ok(())
                    })?;
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
        Ok((feature, option, presets, requires))
    }

    fn option_setters(&self) -> syn::Result<Vec<TokenStream>> {
        let mut setters = Vec::with_capacity(self.plugin_options.len());
        for entry in &self.plugin_options {
            let dependencies = dependency_closure(entry, &self.plugin_options)?;
            let dependents = dependent_closure(entry, &self.plugin_options)?;
            let enable_dependencies = dependencies.iter().map(|dependency| {
                let field = &dependency.field;
                let feature = &dependency.feature;
                quote! {
                    #[cfg(feature = #feature)]
                    {
                        self.#field = true;
                    }
                }
            });
            let disable_dependents = dependents.iter().map(|dependent| {
                let field = &dependent.field;
                let feature = &dependent.feature;
                quote! {
                    #[cfg(feature = #feature)]
                    {
                        self.#field = false;
                    }
                }
            });
            let field = &entry.field;
            let feature = &entry.feature;
            setters.push(quote! {
                #[cfg(feature = #feature)]
                pub const fn #field(mut self, enabled: bool) -> Self {
                    self.#field = enabled;
                    if enabled {
                        #(#enable_dependencies)*
                    } else {
                        #(#disable_dependents)*
                    }
                    self
                }
            });
        }
        Ok(setters)
    }

    fn option_presets(&self) -> Vec<TokenStream> {
        let mut presets = Vec::with_capacity(self.preset_names.len());
        for preset in &self.preset_names {
            let preset_feature = LitStr::new(&preset.to_string(), preset.span());
            let mut enables = Vec::new();
            for entry in &self.plugin_options {
                if !entry.presets.iter().any(|found| found == preset) {
                    continue;
                }
                let field = &entry.field;
                let feature = &entry.feature;
                enables.push(quote! {
                    #[cfg(feature = #feature)]
                    {
                        options = options.#field(true);
                    }
                });
            }
            presets.push(quote! {
                #[cfg(feature = #preset_feature)]
                pub const fn #preset() -> Self {
                    let mut options = Self::none();
                    #(#enables)*
                    options
                }
            });
        }
        presets
    }

    fn options_tokens(&self, options: &Ident) -> syn::Result<TokenStream> {
        let visibility = &self.visibility;
        let option_fields = self.plugin_options.iter().map(|entry| {
            let field = &entry.field;
            let feature = &entry.feature;
            quote! {
                #[cfg(feature = #feature)]
                #field: bool,
            }
        });
        let none_fields = self.plugin_options.iter().map(|entry| {
            let field = &entry.field;
            let feature = &entry.feature;
            quote! {
                #[cfg(feature = #feature)]
                #field: false,
            }
        });
        let option_setters = self.option_setters()?;
        let option_presets = self.option_presets();
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
        })
    }

    fn builder_tokens(&self, ident: &Ident, options: &Ident, builder: &Ident) -> TokenStream {
        let visibility = &self.visibility;
        let builder_setters = self.plugin_options.iter().map(|entry| {
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
        let builder_presets = self.preset_names.iter().map(|preset| {
            let preset_feature = LitStr::new(&preset.to_string(), preset.span());
            quote! {
                #[cfg(feature = #preset_feature)]
                pub const fn #preset(mut self) -> Self {
                    self.options = #options::#preset();
                    self
                }
            }
        });
        quote! {
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
        }
    }

    fn expand(self) -> syn::Result<TokenStream> {
        let ident = &self.ident;
        let options = format_ident!("{ident}Options");
        let builder = format_ident!("{ident}Builder");
        let visibility = &self.visibility;
        let docs = &self.docs;
        let options_tokens = self.options_tokens(&options)?;
        let builder_tokens = self.builder_tokens(ident, &options, &builder);
        let plugin_installs = self.installs.iter().map(|entry| {
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
        Ok(quote! {
            #options_tokens
            #builder_tokens

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
}

pub(crate) fn expand(input: DeriveInput) -> syn::Result<TokenStream> {
    AppPlugin::parse(input)?.expand()
}

fn dependency_closure<'a>(
    option: &'a PluginOption,
    options: &'a [PluginOption],
) -> syn::Result<Vec<&'a PluginOption>> {
    let mut path = vec![option.field.clone()];
    let mut found = Vec::new();
    collect_dependencies(option, options, &mut path, &mut found)?;
    Ok(found)
}

fn collect_dependencies<'a>(
    option: &'a PluginOption,
    options: &'a [PluginOption],
    path: &mut Vec<Ident>,
    found: &mut Vec<&'a PluginOption>,
) -> syn::Result<()> {
    for required in &option.requires {
        if path.iter().any(|field| field == required) {
            return Err(syn::Error::new_spanned(
                required,
                "plugin option dependency cycle",
            ));
        }
        let Some(dependency) = options.iter().find(|entry| entry.field == *required) else {
            return Err(syn::Error::new_spanned(
                required,
                "unknown plugin option dependency",
            ));
        };
        if found.iter().any(|entry| entry.field == dependency.field) {
            continue;
        }
        path.push(required.clone());
        collect_dependencies(dependency, options, path, found)?;
        path.pop();
        found.push(dependency);
    }
    Ok(())
}

fn dependent_closure<'a>(
    option: &'a PluginOption,
    options: &'a [PluginOption],
) -> syn::Result<Vec<&'a PluginOption>> {
    let mut found = Vec::new();
    for candidate in options {
        if dependency_closure(candidate, options)?
            .iter()
            .any(|dependency| dependency.field == option.field)
        {
            found.push(candidate);
        }
    }
    Ok(found)
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
