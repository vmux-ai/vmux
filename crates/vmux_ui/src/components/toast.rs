use dioxus::prelude::*;
use dioxus_primitives::toast::{self, ToastProviderProps};

#[component]
pub fn ToastProvider(props: ToastProviderProps) -> Element {
    rsx! {
        Fragment {
            style { "{include_str!(\"toast.css\")}" }
            toast::ToastProvider {
                default_duration: props.default_duration,
                max_toasts: props.max_toasts,
                render_toast: props.render_toast,
                {props.children}
            }
        }
    }
}
