use bevy::prelude::*;
use tray_icon::menu::{Menu, MenuItem, PredefinedMenuItem};
use tray_icon::{TrayIcon, TrayIconBuilder};

use crate::os_menu::{OsMenuEntry, OsMenuSelect};
#[cfg(feature = "recording")]
use crate::recording::{RecordingControl, RecordingStatus};
use crate::runtime::{HideAllWindowsRequest, QuitRequest, ShowAllWindowsRequest};
use vmux_setting::AppSettings;
use vmux_ui::i18n::Locale;

pub(crate) struct TrayPlugin;

impl Plugin for TrayPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_tray.after(vmux_setting::SettingsLoadSet))
            .add_observer(toggle_tray_visibility)
            .add_observer(quit_from_tray)
            .add_observer(control_recording_from_tray)
            .add_systems(Update, (sync_tray_menu_state, sync_tray_recording));
    }
}

#[derive(Component)]
struct ToggleTrayVisibility;

#[derive(Component)]
struct QuitFromTray;

#[cfg(feature = "recording")]
#[derive(Component)]
struct PauseRecordingFromTray;

#[cfg(feature = "recording")]
#[derive(Component)]
struct ResumeRecordingFromTray;

#[cfg(feature = "recording")]
#[derive(Component)]
struct FinishRecordingFromTray;

struct TrayHandle {
    _tray: TrayIcon,
    toggle: MenuItem,
    quit: MenuItem,
    #[cfg(feature = "recording")]
    pause: MenuItem,
    #[cfg(feature = "recording")]
    resume: MenuItem,
    #[cfg(feature = "recording")]
    done: MenuItem,
    last_any_visible: Option<bool>,
    last_locale: Locale,
    #[cfg(feature = "recording")]
    last_status: Option<RecordingStatus>,
}

fn setup_tray(world: &mut World) {
    let locale = tray_locale(world.resource::<AppSettings>());
    let menu = Menu::new();
    let toggle = MenuItem::new(toggle_label(true, &locale), true, None);
    #[cfg(feature = "recording")]
    let pause = MenuItem::new(locale.translate("tray-pause-recording"), false, None);
    #[cfg(feature = "recording")]
    let resume = MenuItem::new(locale.translate("tray-resume-recording"), false, None);
    #[cfg(feature = "recording")]
    let done = MenuItem::new(locale.translate("tray-finish-recording"), false, None);
    let quit = MenuItem::new(locale.translate("tray-quit"), true, None);
    let toggle_id = toggle.id().0.clone();
    let quit_id = quit.id().0.clone();
    #[cfg(feature = "recording")]
    let pause_id = pause.id().0.clone();
    #[cfg(feature = "recording")]
    let resume_id = resume.id().0.clone();
    #[cfg(feature = "recording")]
    let done_id = done.id().0.clone();

    #[cfg(feature = "recording")]
    let append_result = menu.append_items(&[
        &toggle,
        &PredefinedMenuItem::separator(),
        &pause,
        &resume,
        &done,
        &PredefinedMenuItem::separator(),
        &quit,
    ]);
    #[cfg(not(feature = "recording"))]
    let append_result = menu.append_items(&[&toggle, &PredefinedMenuItem::separator(), &quit]);
    if let Err(e) = append_result {
        tracing::error!(error = %e, "failed to append tray menu items");
        return;
    }

    let icon = load_tray_icon();
    let tray = match TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("Vmux")
        .with_icon(icon)
        .with_icon_as_template(true)
        .build()
    {
        Ok(t) => t,
        Err(e) => {
            tracing::error!(error = %e, "failed to build tray icon");
            return;
        }
    };

    world.insert_non_send(TrayHandle {
        _tray: tray,
        toggle,
        quit,
        #[cfg(feature = "recording")]
        pause,
        #[cfg(feature = "recording")]
        resume,
        #[cfg(feature = "recording")]
        done,
        last_any_visible: None,
        last_locale: locale,
        #[cfg(feature = "recording")]
        last_status: None,
    });
    world.spawn((
        Name::new("Toggle tray visibility"),
        OsMenuEntry::identified(toggle_id),
        ToggleTrayVisibility,
    ));
    world.spawn((
        Name::new("Quit from tray"),
        OsMenuEntry::identified(quit_id),
        QuitFromTray,
    ));
    #[cfg(feature = "recording")]
    world.spawn((
        Name::new("Pause recording from tray"),
        OsMenuEntry::identified(pause_id),
        PauseRecordingFromTray,
    ));
    #[cfg(feature = "recording")]
    world.spawn((
        Name::new("Resume recording from tray"),
        OsMenuEntry::identified(resume_id),
        ResumeRecordingFromTray,
    ));
    #[cfg(feature = "recording")]
    world.spawn((
        Name::new("Finish recording from tray"),
        OsMenuEntry::identified(done_id),
        FinishRecordingFromTray,
    ));
}

fn toggle_tray_visibility(
    trigger: On<OsMenuSelect>,
    menu_items: Query<(), With<ToggleTrayVisibility>>,
    windows: Query<&Window>,
    mut hide_windows: MessageWriter<HideAllWindowsRequest>,
    mut show_windows: MessageWriter<ShowAllWindowsRequest>,
) {
    if !menu_items.contains(trigger.event_target()) {
        return;
    }
    let any_visible = windows.iter().any(|w| w.visible);
    if any_visible {
        hide_windows.write(HideAllWindowsRequest);
    } else {
        show_windows.write(ShowAllWindowsRequest);
    }
}

fn quit_from_tray(
    trigger: On<OsMenuSelect>,
    menu_items: Query<(), With<QuitFromTray>>,
    mut quit: MessageWriter<QuitRequest>,
) {
    if menu_items.contains(trigger.event_target()) {
        quit.write(QuitRequest);
    }
}

#[cfg(feature = "recording")]
fn control_recording_from_tray(
    trigger: On<OsMenuSelect>,
    pause: Query<(), With<PauseRecordingFromTray>>,
    resume: Query<(), With<ResumeRecordingFromTray>>,
    finish: Query<(), With<FinishRecordingFromTray>>,
    mut controls: MessageWriter<RecordingControl>,
) {
    let target = trigger.event_target();
    if pause.contains(target) {
        controls.write(RecordingControl::Pause);
    } else if resume.contains(target) {
        controls.write(RecordingControl::Resume);
    } else if finish.contains(target) {
        controls.write(RecordingControl::Done);
    }
}

#[cfg(not(feature = "recording"))]
fn control_recording_from_tray(_trigger: On<OsMenuSelect>) {}

fn sync_tray_menu_state(
    handle: Option<NonSendMut<TrayHandle>>,
    windows: Query<&Window>,
    settings: Res<AppSettings>,
) {
    let Some(mut handle) = handle else { return };
    let any_visible = windows.iter().any(|w| w.visible);
    if handle.last_any_visible == Some(any_visible) && !settings.is_changed() {
        return;
    }
    let locale = tray_locale(&settings);
    if handle.last_any_visible == Some(any_visible) && handle.last_locale == locale {
        return;
    }
    handle.last_any_visible = Some(any_visible);
    handle.last_locale.clone_from(&locale);
    handle.toggle.set_text(toggle_label(any_visible, &locale));
    handle.quit.set_text(locale.translate("tray-quit"));
    #[cfg(feature = "recording")]
    {
        handle
            .pause
            .set_text(locale.translate("tray-pause-recording"));
        handle
            .resume
            .set_text(locale.translate("tray-resume-recording"));
        handle
            .done
            .set_text(locale.translate("tray-finish-recording"));
    }
}

#[cfg(feature = "recording")]
fn sync_tray_recording(status: Res<RecordingStatus>, handle: Option<NonSendMut<TrayHandle>>) {
    let Some(mut handle) = handle else { return };
    if handle.last_status == Some(*status) {
        return;
    }
    handle.last_status = Some(*status);

    let recording = !matches!(*status, RecordingStatus::Idle);
    let icon = if recording {
        load_tray_icon_recording()
    } else {
        load_tray_icon()
    };
    let _ = handle
        ._tray
        .set_icon_with_as_template(Some(icon), !recording);

    handle
        .pause
        .set_enabled(matches!(*status, RecordingStatus::Recording));
    handle
        .resume
        .set_enabled(matches!(*status, RecordingStatus::Paused));
    handle.done.set_enabled(recording);
}

#[cfg(not(feature = "recording"))]
fn sync_tray_recording() {}

fn toggle_label(any_visible: bool, locale: &Locale) -> String {
    if any_visible {
        locale.translate("tray-close-window")
    } else {
        locale.translate("tray-open-window")
    }
}

fn tray_locale(settings: &AppSettings) -> Locale {
    let locale = Locale::requested(Some(&settings.appearance.locale));
    let directory = vmux_core::profile::config_dir().join("locales");
    let tag = locale.as_str();
    if let Some(source) = [tag, tag.split('-').next().unwrap_or(tag)]
        .into_iter()
        .find_map(|tag| std::fs::read_to_string(directory.join(format!("{tag}.ftl"))).ok())
    {
        let _ = locale.register_catalog(&source);
    }
    locale
}

fn load_tray_icon() -> tray_icon::Icon {
    let rgba = tray_icon_rgba();
    tray_icon::Icon::from_rgba(rgba, 16, 16).expect("valid placeholder rgba")
}

#[cfg(feature = "recording")]
fn load_tray_icon_recording() -> tray_icon::Icon {
    let rgba = tray_icon_recording_rgba();
    tray_icon::Icon::from_rgba(rgba, 16, 16).expect("valid recording rgba")
}

#[cfg(feature = "recording")]
fn tray_icon_recording_rgba() -> Vec<u8> {
    let mut rgba = Vec::with_capacity(16 * 16 * 4);
    let (cx, cy, r) = (7.5_f32, 7.5_f32, 5.5_f32);
    for y in 0..16 {
        for x in 0..16 {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            if dx * dx + dy * dy <= r * r {
                rgba.extend_from_slice(&[224, 38, 38, 255]);
            } else {
                rgba.extend_from_slice(&[0, 0, 0, 0]);
            }
        }
    }
    rgba
}

fn tray_icon_rgba() -> Vec<u8> {
    let mut rgba = Vec::with_capacity(16 * 16 * 4);
    for y in 0_i32..16 {
        for x in 0_i32..16 {
            let dy = y - 3;
            let visible = (0..=10).contains(&dy)
                && ((x - (3 + dy / 2)).abs() <= 1 || (x - (12 - dy / 2)).abs() <= 1);
            if visible {
                rgba.extend_from_slice(&[0, 0, 0, 255]);
            } else {
                rgba.extend_from_slice(&[0, 0, 0, 0]);
            }
        }
    }
    rgba
}

#[cfg(test)]
mod tests {
    #[test]
    fn toggle_label_reflects_visibility() {
        use vmux_ui::i18n::Locale;

        assert_eq!(
            super::toggle_label(true, &Locale::from("en-US")),
            "Close Window"
        );
        assert_eq!(
            super::toggle_label(false, &Locale::from("en-US")),
            "Open Window"
        );
        assert_eq!(
            super::toggle_label(false, &Locale::from("ja")),
            "ウインドウを開く"
        );
    }

    #[test]
    fn tray_icon_has_visible_pixels() {
        let rgba = super::tray_icon_rgba();

        assert_eq!(rgba.len(), 16 * 16 * 4);
        assert!(
            rgba.chunks_exact(4).any(|pixel| pixel[3] != 0),
            "tray icon must not be fully transparent"
        );
    }
}
