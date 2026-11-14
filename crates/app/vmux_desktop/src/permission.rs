use bevy::ecs::system::NonSendMarker;
use bevy::prelude::*;
use bevy_cef::prelude::{MediaPermissionReceiver, resolve_media_permission};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use vmux_setting::AppSettings;
use vmux_ui::i18n::{Locale, TranslationValue};

#[cfg(target_os = "macos")]
use objc2::MainThreadMarker;
#[cfg(target_os = "macos")]
use objc2_app_kit::{NSAlert, NSAlertFirstButtonReturn, NSAlertStyle, NSImage};
#[cfg(target_os = "macos")]
use objc2_foundation::NSString;

pub struct PermissionsPlugin;

impl Plugin for PermissionsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_media_store).add_systems(
            Update,
            (drain_media_requests, persist_media_permissions).chain(),
        );
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PermissionDecision {
    Allow,
    Block,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OriginPermissions {
    #[serde(default)]
    pub camera: Option<PermissionDecision>,
    #[serde(default)]
    pub microphone: Option<PermissionDecision>,
    #[serde(default)]
    pub screen: Option<PermissionDecision>,
}

#[derive(Component, Clone, Debug, Default, Serialize, Deserialize)]
pub struct MediaPermissionStore {
    #[serde(default)]
    origins: HashMap<String, OriginPermissions>,
}

#[derive(Component)]
struct MediaPermissionDirty;

fn spawn_media_store(mut commands: Commands) {
    let store: MediaPermissionStore = std::fs::read_to_string(store_path())
        .ok()
        .and_then(|text| ron::from_str(&text).ok())
        .unwrap_or_default();
    commands.spawn((Name::new("Media permissions"), store));
}

fn store_path() -> PathBuf {
    vmux_ecs::profile::ProfilePaths::current()
        .profile()
        .join("media_permissions.ron")
}

impl MediaPermissionStore {
    fn decision_for(&self, origin: &str, categories: RequestCategories) -> Resolution {
        let entry = self.origins.get(origin);
        let states = [
            categories.state(categories.camera, entry.and_then(|e| e.camera)),
            categories.state(categories.microphone, entry.and_then(|e| e.microphone)),
            categories.state(categories.screen, entry.and_then(|e| e.screen)),
        ];
        if states
            .iter()
            .any(|state| matches!(state, CategoryState::Undecided))
        {
            Resolution::Prompt
        } else if states
            .iter()
            .any(|state| matches!(state, CategoryState::Block))
        {
            Resolution::Deny
        } else {
            Resolution::Grant
        }
    }

    fn record(&mut self, origin: &str, categories: RequestCategories, allow: bool) {
        let decision = if allow {
            PermissionDecision::Allow
        } else {
            PermissionDecision::Block
        };
        let entry = self.origins.entry(origin.to_string()).or_default();
        if categories.camera {
            entry.camera = Some(decision);
        }
        if categories.microphone {
            entry.microphone = Some(decision);
        }
        if categories.screen {
            entry.screen = Some(decision);
        }
    }
}

#[derive(Clone, Copy)]
struct RequestCategories {
    camera: bool,
    microphone: bool,
    screen: bool,
}

impl RequestCategories {
    fn host_allowed(&self) -> bool {
        (!self.camera || vmux_profile::Product::camera())
            && (!self.microphone || vmux_profile::Product::microphone())
            && (!self.screen || vmux_profile::Product::screen())
    }

    fn state(&self, requested: bool, stored: Option<PermissionDecision>) -> CategoryState {
        if !requested {
            CategoryState::NotRequested
        } else {
            match stored {
                Some(PermissionDecision::Allow) => CategoryState::Allow,
                Some(PermissionDecision::Block) => CategoryState::Block,
                None => CategoryState::Undecided,
            }
        }
    }

    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    fn phrase(&self, locale: &Locale) -> String {
        let kind =
            i64::from(self.camera) + 2 * i64::from(self.microphone) + 4 * i64::from(self.screen);
        locale.translate_with(
            "media-permission-devices",
            &[("kind", TranslationValue::Number(kind))],
        )
    }

    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    fn symbol(&self) -> &'static str {
        if self.camera {
            "video.fill"
        } else if self.microphone {
            "mic.fill"
        } else {
            "rectangle.on.rectangle"
        }
    }
}

enum CategoryState {
    NotRequested,
    Undecided,
    Allow,
    Block,
}

enum Resolution {
    Grant,
    Deny,
    Prompt,
}

fn drain_media_requests(
    receiver: Res<MediaPermissionReceiver>,
    mut store: Single<(Entity, &mut MediaPermissionStore)>,
    mut commands: Commands,
    _main_thread: NonSendMarker,
    settings: Res<AppSettings>,
) {
    let (entity, store) = &mut *store;
    while let Ok(request) = receiver.0.try_recv() {
        let categories = RequestCategories {
            camera: request.wants_camera,
            microphone: request.wants_microphone,
            screen: request.wants_screen,
        };
        let resolution = if categories.host_allowed() {
            store.decision_for(&request.origin, categories)
        } else {
            Resolution::Deny
        };
        let allow = match resolution {
            Resolution::Grant => true,
            Resolution::Deny => false,
            Resolution::Prompt => match prompt_native(&request.origin, categories, &settings) {
                Some(decision) => {
                    store.record(&request.origin, categories, decision);
                    commands.entity(*entity).insert(MediaPermissionDirty);
                    decision
                }
                None => false,
            },
        };
        resolve_media_permission(request.request_id, allow);
    }
}

fn persist_media_permissions(
    stores: Query<(Entity, &MediaPermissionStore), With<MediaPermissionDirty>>,
    mut commands: Commands,
) {
    for (entity, store) in &stores {
        let path = store_path();
        let Some(parent) = path.parent() else {
            continue;
        };
        if std::fs::create_dir_all(parent).is_err() {
            continue;
        }
        let Ok(text) = ron::ser::to_string_pretty(store, ron::ser::PrettyConfig::default()) else {
            continue;
        };
        if std::fs::write(path, text).is_ok() {
            commands.entity(entity).remove::<MediaPermissionDirty>();
        }
    }
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn permission_host(origin: &str) -> &str {
    let without_scheme = origin
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(origin);
    without_scheme.split('/').next().unwrap_or(without_scheme)
}

#[cfg(target_os = "macos")]
fn prompt_native(
    origin: &str,
    categories: RequestCategories,
    settings: &AppSettings,
) -> Option<bool> {
    let mtm = MainThreadMarker::new()?;
    let locale = Locale::requested(Some(&settings.appearance.locale));
    let devices = categories.phrase(&locale);
    let alert = NSAlert::new(mtm);
    alert.setAlertStyle(NSAlertStyle::Informational);
    alert.setMessageText(&NSString::from_str(&locale.translate_with(
        "media-permission-title",
        &[
            ("host", TranslationValue::String(permission_host(origin))),
            ("devices", TranslationValue::String(&devices)),
        ],
    )));
    alert.setInformativeText(&NSString::from_str(&locale.translate_with(
        "media-permission-remember",
        &[(
            "product",
            TranslationValue::String(vmux_profile::Product::name()),
        )],
    )));
    if let Some(icon) = NSImage::imageWithSystemSymbolName_accessibilityDescription(
        &NSString::from_str(categories.symbol()),
        None,
    ) {
        unsafe { alert.setIcon(Some(&icon)) };
    }
    alert.addButtonWithTitle(&NSString::from_str(
        &locale.translate("media-permission-allow"),
    ));
    alert.addButtonWithTitle(&NSString::from_str(
        &locale.translate("media-permission-block"),
    ));
    Some(alert.runModal() == NSAlertFirstButtonReturn)
}

#[cfg(not(target_os = "macos"))]
fn prompt_native(
    _origin: &str,
    _categories: RequestCategories,
    _settings: &AppSettings,
) -> Option<bool> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cats(camera: bool, microphone: bool, screen: bool) -> RequestCategories {
        RequestCategories {
            camera,
            microphone,
            screen,
        }
    }

    fn store_with(origin: &str, perms: OriginPermissions) -> MediaPermissionStore {
        let mut store = MediaPermissionStore::default();
        store.origins.insert(origin.to_string(), perms);
        store
    }

    #[test]
    fn unknown_origin_prompts() {
        let store = MediaPermissionStore::default();
        assert!(matches!(
            store.decision_for("https://meet.google.com", cats(false, true, false)),
            Resolution::Prompt
        ));
    }

    #[test]
    fn all_requested_categories_allowed_grants() {
        let store = store_with(
            "https://meet.google.com",
            OriginPermissions {
                camera: Some(PermissionDecision::Allow),
                microphone: Some(PermissionDecision::Allow),
                screen: None,
            },
        );
        assert!(matches!(
            store.decision_for("https://meet.google.com", cats(true, true, false)),
            Resolution::Grant
        ));
    }

    #[test]
    fn any_blocked_requested_category_denies() {
        let store = store_with(
            "https://meet.google.com",
            OriginPermissions {
                camera: Some(PermissionDecision::Allow),
                microphone: Some(PermissionDecision::Block),
                screen: None,
            },
        );
        assert!(matches!(
            store.decision_for("https://meet.google.com", cats(true, true, false)),
            Resolution::Deny
        ));
    }

    #[test]
    fn stored_decision_for_other_category_still_prompts() {
        let store = store_with(
            "https://meet.google.com",
            OriginPermissions {
                camera: Some(PermissionDecision::Allow),
                microphone: None,
                screen: None,
            },
        );
        assert!(matches!(
            store.decision_for("https://meet.google.com", cats(true, true, false)),
            Resolution::Prompt
        ));
    }

    #[test]
    fn record_sets_only_requested_categories() {
        let mut store = MediaPermissionStore::default();
        store.record("https://meet.google.com", cats(true, true, false), true);
        let entry = store.origins.get("https://meet.google.com").unwrap();
        assert_eq!(entry.camera, Some(PermissionDecision::Allow));
        assert_eq!(entry.microphone, Some(PermissionDecision::Allow));
        assert_eq!(entry.screen, None);
    }

    #[test]
    fn permission_host_strips_scheme_and_path() {
        assert_eq!(
            permission_host("https://meet.google.com/abc"),
            "meet.google.com"
        );
        assert_eq!(permission_host("meet.google.com"), "meet.google.com");
    }
}
