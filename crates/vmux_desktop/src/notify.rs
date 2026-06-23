use bevy::prelude::*;
use vmux_core::notify::OsNotify;

#[cfg(target_os = "macos")]
pub fn request_notification_auth() {
    use block2::RcBlock;
    use objc2::runtime::Bool;
    use objc2_foundation::NSError;
    use objc2_user_notifications::UNAuthorizationOptions;

    let Some(center) = current_center() else {
        return;
    };
    let handler = RcBlock::new(|_granted: Bool, _error: *mut NSError| {});
    center.requestAuthorizationWithOptions_completionHandler(
        UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound,
        &handler,
    );
}

#[cfg(target_os = "macos")]
fn current_center()
-> Option<objc2::rc::Retained<objc2_user_notifications::UNUserNotificationCenter>> {
    use objc2_foundation::NSBundle;
    use objc2_user_notifications::UNUserNotificationCenter;

    NSBundle::mainBundle().bundleIdentifier()?;
    Some(UNUserNotificationCenter::currentNotificationCenter())
}

#[cfg(target_os = "macos")]
pub fn post_os_notifications(mut reader: MessageReader<OsNotify>) {
    let events: Vec<OsNotify> = reader.read().cloned().collect();
    if events.is_empty() {
        return;
    }
    match current_center() {
        Some(center) => post_native(&center, &events),
        None => {
            for ev in &events {
                osascript_notify(&ev.title, &ev.body);
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn post_native(center: &objc2_user_notifications::UNUserNotificationCenter, events: &[OsNotify]) {
    use objc2_foundation::NSString;
    use objc2_user_notifications::{
        UNMutableNotificationContent, UNNotificationRequest, UNNotificationSound,
    };
    use std::sync::atomic::{AtomicU64, Ordering};

    static NOTIF_SEQ: AtomicU64 = AtomicU64::new(0);
    for ev in events {
        let content = UNMutableNotificationContent::new();
        content.setTitle(&NSString::from_str(&ev.title));
        if !ev.body.is_empty() {
            content.setBody(&NSString::from_str(&ev.body));
        }
        content.setSound(Some(&UNNotificationSound::defaultSound()));
        let seq = NOTIF_SEQ.fetch_add(1, Ordering::Relaxed);
        let id = NSString::from_str(&format!("vmux-{seq}"));
        let request =
            UNNotificationRequest::requestWithIdentifier_content_trigger(&id, &content, None);
        center.addNotificationRequest_withCompletionHandler(&request, None);
    }
}

#[cfg(target_os = "macos")]
fn osascript_notify(title: &str, body: &str) {
    fn esc(s: &str) -> String {
        s.replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', " ")
    }
    let script = format!(
        "display notification \"{}\" with title \"{}\" sound name \"default\"",
        esc(body),
        esc(title)
    );
    if let Ok(mut child) = std::process::Command::new("osascript")
        .arg("-e")
        .arg(script)
        .spawn()
    {
        std::thread::spawn(move || {
            let _ = child.wait();
        });
    }
}

#[cfg(not(target_os = "macos"))]
pub fn request_notification_auth() {}

#[cfg(not(target_os = "macos"))]
pub fn post_os_notifications(mut reader: MessageReader<OsNotify>) {
    for _ in reader.read() {}
}
