use proc_macro2::TokenStream;
use quote::{ToTokens, quote};
use serde::Deserialize;
use std::collections::HashSet;
use std::path::PathBuf;
use syn::parse::Parser;
use syn::punctuated::Punctuated;
use syn::{Data, DeriveInput, Expr, Ident, LitStr, Meta, Path, Type};

pub(crate) struct Catalog {
    item: DeriveInput,
    state: Type,
    entries: Vec<Entry>,
    categories: Vec<LitStr>,
}

impl Catalog {
    pub(crate) fn parse(args: TokenStream, mut item: DeriveInput) -> syn::Result<Self> {
        let args = Punctuated::<Meta, syn::Token![,]>::parse_terminated.parse2(args)?;
        let mut state = None;
        let mut categories = Vec::new();
        for arg in args {
            let Meta::NameValue(value) = arg else {
                return Err(syn::Error::new_spanned(arg, "expected state = Type"));
            };
            if value.path.is_ident("categories") {
                let Expr::Array(array) = value.value else {
                    return Err(syn::Error::new_spanned(
                        value.value,
                        "categories must be an array",
                    ));
                };
                for value in array.elems {
                    categories.push(syn::parse2(quote! { #value })?);
                }
                continue;
            }
            if !value.path.is_ident("state") {
                return Err(syn::Error::new_spanned(
                    value.path,
                    "unknown catalog option",
                ));
            }
            state = Some(syn::parse2::<Type>(value.value.to_token_stream())?);
        }
        let state = state
            .ok_or_else(|| syn::Error::new_spanned(&item.ident, "catalog requires state = Type"))?;
        let Data::Enum(data) = &mut item.data else {
            return Err(syn::Error::new_spanned(
                &item.ident,
                "story catalog requires an enum",
            ));
        };
        let mut entries = Vec::new();
        let mut ids = HashSet::new();
        for variant in &mut data.variants {
            if !matches!(variant.fields, syn::Fields::Unit) {
                return Err(syn::Error::new_spanned(
                    &variant.ident,
                    "story variants must be unit variants",
                ));
            }
            let position = variant
                .attrs
                .iter()
                .position(|attr| attr.path().is_ident("story"))
                .ok_or_else(|| {
                    syn::Error::new_spanned(&variant.ident, "variant requires #[story(...)]")
                })?;
            let attr = variant.attrs.remove(position);
            let entry = Entry::parse(variant.ident.clone(), attr)?;
            if !ids.insert(entry.id.value()) {
                return Err(syn::Error::new_spanned(&entry.id, "duplicate story ID"));
            }
            entries.push(entry);
        }
        Ok(Self {
            item,
            state,
            entries,
            categories,
        })
    }

    pub(crate) fn expand(self) -> TokenStream {
        let item = self.item;
        let ident = &item.ident;
        let state = self.state;
        let count = self.entries.len();
        let mut variants = Vec::new();
        let mut names = Vec::new();
        let mut ids = Vec::new();
        let mut components = Vec::new();
        let mut controls = Vec::new();
        let mut renderers = Vec::new();
        let mut metadata_sources = Vec::new();
        let mut metadata_paths = Vec::new();
        let mut arg_checks = Vec::new();
        let mut category_names = Vec::new();
        let mut categories = self.categories;
        for entry in self.entries {
            if !categories
                .iter()
                .any(|value| value.value() == entry.category.value())
            {
                categories.push(entry.category.clone());
            }
            category_names.push(entry.category);
            variants.push(entry.variant);
            names.push(entry.name);
            ids.push(entry.id);
            components.push(entry.components);
            controls.push(entry.controls);
            renderers.push(entry.component);
            metadata_sources.push(entry.metadata_source);
            metadata_paths.push(entry.metadata_path);
            arg_checks.extend(entry.arg_checks);
        }
        quote! {
            #item
            #(const _: &str = include_str!(#metadata_paths);)*
            const _: fn() = || { #(let _: crate::ArgsPatch = #arg_checks;)* };
            impl #ident {
                pub const ALL: [Self; #count] = [#(Self::#variants),*];
                pub const CATEGORIES: &'static [&'static str] = &[#(#categories),*];
                pub fn category(self) -> &'static str { match self { #(Self::#variants => #category_names),* } }
                pub fn name(self) -> &'static str { match self { #(Self::#variants => #names),* } }
                pub fn slug(self) -> &'static str { match self { #(Self::#variants => #ids),* } }
                pub fn component_names(self) -> &'static str { match self { #(Self::#variants => #components),* } }
                pub fn control_names(self) -> &'static [&'static str] { match self { #(Self::#variants => &[#(#controls),*]),* } }
                pub fn metadata_source(self) -> &'static str { match self { #(Self::#variants => #metadata_sources),* } }
                pub fn url(self) -> String { format!("vmux://ui/{}", self.slug()) }
                pub fn from_url(url: &str) -> Option<Self> {
                    let path = url.split(['?', '#']).next()?.strip_prefix("vmux://ui")?;
                    if path.is_empty() || path == "/" { return Some(Self::default()); }
                    let slug = path.strip_prefix('/')?.trim_end_matches('/');
                    Self::ALL.into_iter().find(|story| slug == story.slug())
                }
            }
            #[cfg(ui)]
            #[dioxus::prelude::component]
            #[allow(non_snake_case)]
            pub(crate) fn RegisteredStory(state: #state) -> ::dioxus::prelude::Element {
                ::dioxus::prelude::rsx! {
                    match state.story { #(#ident::#variants => ::dioxus::prelude::rsx! { #renderers { state } }),* }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_quote;

    #[test]
    fn duplicate_story_ids_are_rejected() {
        let item = parse_quote! {
            enum Story {
                #[story(id = "button", component = crate::Button)]
                First,
                #[story(id = "button", component = crate::Other)]
                Second,
            }
        };
        assert!(Catalog::parse(quote! { state = Args }, item).is_err());
    }

    #[test]
    fn stories_require_a_renderer_and_a_unit_variant() {
        for item in [
            parse_quote! { enum Story { #[story(id = "button")] Button } },
            parse_quote! { enum Story { #[story(id = "button", component = crate::Button)] Button(String) } },
        ] {
            assert!(Catalog::parse(quote! { state = Args }, item).is_err());
        }
    }
}

struct Entry {
    variant: Ident,
    id: LitStr,
    name: LitStr,
    category: LitStr,
    components: LitStr,
    controls: Vec<LitStr>,
    component: Path,
    metadata_source: LitStr,
    metadata_path: LitStr,
    arg_checks: Vec<TokenStream>,
}

impl Entry {
    fn parse(variant: Ident, attr: syn::Attribute) -> syn::Result<Self> {
        let values = attr.parse_args_with(Punctuated::<Meta, syn::Token![,]>::parse_terminated)?;
        let mut entry = Self {
            id: LitStr::new("", variant.span()),
            name: LitStr::new(&variant.to_string(), variant.span()),
            category: LitStr::new("gallery-category-primitives", variant.span()),
            components: LitStr::new(&variant.to_string(), variant.span()),
            controls: Vec::new(),
            component: syn::parse_quote!(Self),
            metadata_source: LitStr::new("", variant.span()),
            metadata_path: LitStr::new(
                concat!(env!("CARGO_MANIFEST_DIR"), "/src/lib.rs"),
                variant.span(),
            ),
            arg_checks: Vec::new(),
            variant,
        };
        let mut id = None;
        let mut component = None;
        let mut metadata = None;
        for value in values {
            let Meta::NameValue(value) = value else {
                return Err(syn::Error::new_spanned(
                    value,
                    "expected story metadata key = value",
                ));
            };
            if value.path.is_ident("component") {
                let Expr::Path(path) = value.value else {
                    return Err(syn::Error::new_spanned(
                        value.value,
                        "component must be a path",
                    ));
                };
                component = Some(path.path);
                continue;
            }
            if value.path.is_ident("controls") {
                let Expr::Array(array) = value.value else {
                    return Err(syn::Error::new_spanned(
                        value.value,
                        "controls must be an array",
                    ));
                };
                for value in array.elems {
                    entry.controls.push(syn::parse2(quote! { #value })?);
                }
                continue;
            }
            let text: LitStr = syn::parse2(value.value.to_token_stream())?;
            if value.path.is_ident("metadata") {
                metadata = Some(text);
            } else if value.path.is_ident("id") {
                id = Some(text);
            } else if value.path.is_ident("name") {
                entry.name = text;
            } else if value.path.is_ident("category") {
                entry.category = text;
            } else if value.path.is_ident("components") {
                entry.components = text;
            } else {
                return Err(syn::Error::new_spanned(
                    value.path,
                    "unknown story metadata",
                ));
            }
        }
        if let Some(file) = metadata {
            id = Some(entry.load_metadata(file)?);
        }
        entry.id =
            id.ok_or_else(|| syn::Error::new_spanned(&entry.variant, "story requires id"))?;
        entry.component = component
            .ok_or_else(|| syn::Error::new_spanned(&entry.variant, "story requires component"))?;
        Ok(entry)
    }

    fn load_metadata(&mut self, file: LitStr) -> syn::Result<LitStr> {
        let path =
            PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap()).join(file.value());
        let body = std::fs::read_to_string(&path)
            .map_err(|error| syn::Error::new_spanned(&file, error.to_string()))?;
        let definition: Definition = ron::from_str(&body)
            .map_err(|error| syn::Error::new_spanned(&file, error.to_string()))?;
        self.arg_checks = definition.validate()?;
        self.name = LitStr::new(&definition.title, file.span());
        self.category = LitStr::new(&definition.category, file.span());
        self.components = LitStr::new(&definition.components.join(" · "), file.span());
        self.controls = definition
            .controls
            .iter()
            .map(|value| LitStr::new(value, file.span()))
            .collect();
        self.metadata_source = LitStr::new(&body, file.span());
        self.metadata_path = LitStr::new(&path.to_string_lossy(), file.span());
        Ok(LitStr::new(&definition.id, file.span()))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Definition {
    id: String,
    title: String,
    category: String,
    components: Vec<String>,
    controls: Vec<String>,
    defaults: ron::Value,
    presets: Vec<Preset>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Preset {
    name: String,
    args: ron::Value,
}

impl Definition {
    fn validate(&self) -> syn::Result<Vec<TokenStream>> {
        let mut names = HashSet::new();
        let mut checks = vec![Args(&self.defaults).tokens()?];
        for preset in &self.presets {
            if preset.name.trim().is_empty() || !names.insert(preset.name.as_str()) {
                return Err(syn::Error::new(
                    proc_macro2::Span::call_site(),
                    "preset names must be nonempty and unique",
                ));
            }
            checks.push(Args(&preset.args).tokens()?);
        }
        Ok(checks)
    }
}

struct Args<'a>(&'a ron::Value);

impl Args<'_> {
    fn tokens(&self) -> syn::Result<TokenStream> {
        let mut fields = Vec::new();
        match self.0 {
            ron::Value::Unit => {}
            ron::Value::Map(map) => {
                for (key, value) in map.iter() {
                    let ron::Value::String(key) = key else {
                        return Err(syn::Error::new(
                            proc_macro2::Span::call_site(),
                            "args require named fields",
                        ));
                    };
                    let field: Ident = syn::parse_str(key)?;
                    let value = match value {
                        ron::Value::Bool(value) => quote! { #value },
                        ron::Value::String(value) => quote! { #value.to_string() },
                        ron::Value::Number(value) => {
                            let value = (*value).into_f64();
                            if !value.is_finite()
                                || key == "cursor"
                                    && (value < 0.0 || value.fract() != 0.0 || value > 16.0)
                            {
                                return Err(syn::Error::new(
                                    proc_macro2::Span::call_site(),
                                    "numeric args require finite, valid conditions",
                                ));
                            }
                            if key == "cursor" {
                                let value = proc_macro2::Literal::usize_unsuffixed(value as usize);
                                quote! { #value }
                            } else {
                                let value = proc_macro2::Literal::f64_unsuffixed(value);
                                quote! { #value }
                            }
                        }
                        _ => {
                            return Err(syn::Error::new(
                                proc_macro2::Span::call_site(),
                                "args support boolean, string and numeric conditions",
                            ));
                        }
                    };
                    fields.push(quote! { #field: Some(#value) });
                }
            }
            _ => {
                return Err(syn::Error::new(
                    proc_macro2::Span::call_site(),
                    "args require a named condition map",
                ));
            }
        }
        Ok(quote! { crate::ArgsPatch { #(#fields,)* ..Default::default() } })
    }
}
