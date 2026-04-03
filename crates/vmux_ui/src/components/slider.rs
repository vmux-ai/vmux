use dioxus::prelude::*;
use dioxus_primitives::slider::{
    self, SliderProps, SliderRangeProps, SliderThumbProps, SliderTrackProps,
};

const SLIDER: &str = "relative flex w-[200px] touch-none items-center py-2 data-[orientation=vertical]:h-[200px] data-[orientation=vertical]:w-auto data-[orientation=vertical]:flex-col data-[disabled=true]:cursor-not-allowed data-[disabled=true]:opacity-50";

const SLIDER_TRACK: &str = "relative box-border h-2 flex-1 rounded-full bg-muted data-[orientation=vertical]:h-full data-[orientation=vertical]:w-1";

const SLIDER_RANGE: &str =
    "absolute h-full rounded-full bg-primary data-[orientation=vertical]:w-full";

const SLIDER_THUMB: &str = "absolute top-1/2 block size-4 -translate-x-1/2 -translate-y-1/2 cursor-pointer rounded-full border border-primary bg-card transition-[box-shadow] duration-150 hover:shadow-[0_0_0_4px_color-mix(in_oklab,var(--primary)_50%,transparent)] focus-visible:shadow-[0_0_0_4px_color-mix(in_oklab,var(--primary)_50%,transparent)] data-[dragging=true]:shadow-[0_0_0_4px_color-mix(in_oklab,var(--primary)_50%,transparent)] data-[orientation=vertical]:left-1/2 data-[orientation=vertical]:top-auto data-[orientation=vertical]:translate-x-[-50%] data-[orientation=vertical]:translate-y-1/2 data-[disabled=true]:cursor-not-allowed";

#[component]
pub fn Slider(props: SliderProps) -> Element {
    rsx! {
        slider::Slider {
            class: SLIDER,
            value: props.value,
            default_value: props.default_value,
            min: props.min,
            max: props.max,
            step: props.step,
            disabled: props.disabled,
            horizontal: props.horizontal,
            inverted: props.inverted,
            on_value_change: props.on_value_change,
            label: props.label,
            attributes: props.attributes,
            {props.children}
        }
    }
}

#[component]
pub fn SliderTrack(props: SliderTrackProps) -> Element {
    rsx! {
        slider::SliderTrack {
            class: SLIDER_TRACK,
            attributes: props.attributes,
            {props.children}
        }
    }
}

#[component]
pub fn SliderRange(props: SliderRangeProps) -> Element {
    rsx! {
        slider::SliderRange {
            class: SLIDER_RANGE,
            attributes: props.attributes,
            {props.children}
        }
    }
}

#[component]
pub fn SliderThumb(props: SliderThumbProps) -> Element {
    rsx! {
        slider::SliderThumb {
            class: SLIDER_THUMB,
            index: props.index,
            attributes: props.attributes,
            {props.children}
        }
    }
}
