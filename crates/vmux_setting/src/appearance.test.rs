use super::*;

#[test]
fn explicit_modes_ignore_os() {
    assert_eq!(
        resolve(ColorScheme::Light, Some(ResolvedScheme::Dark)),
        ResolvedScheme::Light
    );
    assert_eq!(
        resolve(ColorScheme::Dark, Some(ResolvedScheme::Light)),
        ResolvedScheme::Dark
    );
}

#[test]
fn device_follows_os_and_defaults_dark() {
    assert_eq!(
        resolve(ColorScheme::Device, Some(ResolvedScheme::Light)),
        ResolvedScheme::Light
    );
    assert_eq!(
        resolve(ColorScheme::Device, Some(ResolvedScheme::Dark)),
        ResolvedScheme::Dark
    );
    assert_eq!(resolve(ColorScheme::Device, None), ResolvedScheme::Dark);
}
