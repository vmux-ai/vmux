use super::map_color_scheme;
use bevy_cef::prelude::CefColorMode;
use vmux_setting::ColorScheme;

#[test]
fn maps_color_scheme_to_cef_mode() {
    assert_eq!(map_color_scheme(ColorScheme::Light), CefColorMode::Light);
    assert_eq!(map_color_scheme(ColorScheme::Dark), CefColorMode::Dark);
    assert_eq!(map_color_scheme(ColorScheme::Device), CefColorMode::System);
}
