use proc_macro::TokenStream;
use quote::quote;
use syn::parse::Parser;
use syn::punctuated::Punctuated;
use syn::{DeriveInput, Meta, Token, Type, parse_macro_input};

mod app_plugin;
mod bin_event;
mod command;
mod contract;
mod page;
mod service_message;
mod string_id;
mod tool_input;
mod ui_event_variants;
mod ui_state;
mod variant_names;

#[proc_macro_attribute]
pub fn app_plugin(_args: TokenStream, input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match app_plugin::expand(input) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.to_compile_error().into(),
    }
}

#[proc_macro_attribute]
pub fn contract(args: TokenStream, input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match contract::expand(args.into(), input) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.to_compile_error().into(),
    }
}

#[proc_macro_attribute]
pub fn command(args: TokenStream, input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match command::expand(args.into(), input) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.to_compile_error().into(),
    }
}

#[proc_macro_attribute]
pub fn host_event(args: TokenStream, input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match bin_event::expand(args.into(), input, bin_event::Direction::Host) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.to_compile_error().into(),
    }
}

#[proc_macro_attribute]
pub fn ui_event(args: TokenStream, input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match bin_event::expand(args.into(), input, bin_event::Direction::Ui) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.to_compile_error().into(),
    }
}

#[proc_macro_attribute]
pub fn agent(args: TokenStream, input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match bin_event::expand(args.into(), input, bin_event::Direction::Agent) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.to_compile_error().into(),
    }
}

#[proc_macro_attribute]
pub fn ui_event_variants(args: TokenStream, input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match ui_event_variants::expand(args.into(), input) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.to_compile_error().into(),
    }
}

#[proc_macro_attribute]
pub fn bidirectional_event(args: TokenStream, input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match bin_event::expand(args.into(), input, bin_event::Direction::Both) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.to_compile_error().into(),
    }
}

#[proc_macro_attribute]
pub fn ui_state(args: TokenStream, input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let args = match Punctuated::<Meta, Token![,]>::parse_terminated.parse(args) {
        Ok(args) => args,
        Err(error) => return error.to_compile_error().into(),
    };
    let mut event_args = Punctuated::<Meta, Token![,]>::new();
    let mut patch = None::<Type>;
    for arg in args {
        let Meta::NameValue(value) = &arg else {
            event_args.push(arg);
            continue;
        };
        if !value.path.is_ident("patch") {
            event_args.push(arg);
            continue;
        }
        if patch.is_some() {
            return syn::Error::new_spanned(value, "duplicate patch")
                .to_compile_error()
                .into();
        }
        let patch_value = &value.value;
        patch = match syn::parse2(quote!(#patch_value)) {
            Ok(patch) => Some(patch),
            Err(error) => return error.to_compile_error().into(),
        };
    }
    let state = match ui_state::derive_state(&input, patch.as_ref()) {
        Ok(tokens) => tokens,
        Err(error) => return error.to_compile_error().into(),
    };
    match bin_event::expand(quote!(#event_args), input, bin_event::Direction::Host) {
        Ok(event) => quote!(#event #state).into(),
        Err(error) => error.to_compile_error().into(),
    }
}

#[proc_macro_attribute]
pub fn ui_state_patch(args: TokenStream, input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let patch = match ui_state::derive_patch(&input) {
        Ok(tokens) => tokens,
        Err(error) => return error.to_compile_error().into(),
    };
    match contract::expand(args.into(), input) {
        Ok(contract) => quote!(#contract #patch).into(),
        Err(error) => error.to_compile_error().into(),
    }
}

#[proc_macro_attribute]
pub fn page(args: TokenStream, input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match page::expand(args.into(), input) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.to_compile_error().into(),
    }
}

#[proc_macro_attribute]
pub fn service_message(args: TokenStream, input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match service_message::expand(args.into(), input) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.to_compile_error().into(),
    }
}

#[proc_macro_attribute]
pub fn string_id(_args: TokenStream, input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match string_id::expand(input) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.to_compile_error().into(),
    }
}

#[proc_macro_attribute]
pub fn input(_args: TokenStream, input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match tool_input::expand(input) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.to_compile_error().into(),
    }
}

#[proc_macro_attribute]
pub fn variant_names(_args: TokenStream, input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match variant_names::expand(input) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.to_compile_error().into(),
    }
}
