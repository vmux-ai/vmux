use bevy::prelude::*;
use vmux_ecs::notify::OsNotify;

#[cfg(target_os = "macos")]
use block2::RcBlock;
#[cfg(target_os = "macos")]
use objc2::rc::Retained;
#[cfg(target_os = "macos")]
use objc2::runtime::Bool;
#[cfg(target_os = "macos")]
use objc2_foundation::NSBundle;
#[cfg(target_os = "macos")]
use objc2_foundation::NSError;
#[cfg(target_os = "macos")]
use objc2_foundation::NSString;
#[cfg(target_os = "macos")]
use objc2_user_notifications::UNAuthorizationOptions;
#[cfg(target_os = "macos")]
use objc2_user_notifications::UNUserNotificationCenter;
#[cfg(target_os = "macos")]
use objc2_user_notifications::{
    UNMutableNotificationContent, UNNotificationRequest, UNNotificationSound,
};

pub(crate) struct NotificationPlugin;

impl Plugin for NotificationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, (spawn_sequence, request_notification_auth))
            .add_systems(Update, post_os_notifications);
    }
}

#[derive(Component, Default)]
struct NotificationSequence(u64);

impl NotificationSequence {
    #[cfg(target_os = "macos")]
    fn next_id(&mut self) -> String {
        let seq = self.0;
        self.0 = self.0.wrapping_add(1);
        format!("vmux-{seq}")
    }
}

fn spawn_sequence(mut commands: Commands) {
    commands.spawn((
        Name::new("OS notification sequence"),
        NotificationSequence::default(),
    ));
}

fn request_notification_auth() {
    #[cfg(target_os = "macos")]
    NativeNotifier::request_auth();
}

fn post_os_notifications(
    mut reader: MessageReader<OsNotify>,
    mut sequence: Single<&mut NotificationSequence>,
) {
    #[cfg(target_os = "macos")]
    {
        let events: Vec<OsNotify> = reader.read().cloned().collect();
        if events.is_empty() {
            return;
        }
        NativeNotifier::post(&events, &mut sequence);
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = &mut sequence;
        for _ in reader.read() {}
    }
}

#[cfg(target_os = "macos")]
struct NativeNotifier;

#[cfg(target_os = "macos")]
impl NativeNotifier {
    fn request_auth() {
        let Some(center) = Self::current_center() else {
            return;
        };
        let handler = RcBlock::new(|_granted: Bool, _error: *mut NSError| {});
        center.requestAuthorizationWithOptions_completionHandler(
            UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound,
            &handler,
        );
    }

    fn current_center() -> Option<Retained<UNUserNotificationCenter>> {
        NSBundle::mainBundle().bundleIdentifier()?;
        Some(UNUserNotificationCenter::currentNotificationCenter())
    }

    fn post(events: &[OsNotify], sequence: &mut NotificationSequence) {
        match Self::current_center() {
            Some(center) => {
                for ev in events {
                    let content = UNMutableNotificationContent::new();
                    content.setTitle(&NSString::from_str(&ev.title));
                    if !ev.body.is_empty() {
                        content.setBody(&NSString::from_str(&ev.body));
                    }
                    content.setSound(Some(&UNNotificationSound::defaultSound()));
                    let id = NSString::from_str(&sequence.next_id());
                    let request = UNNotificationRequest::requestWithIdentifier_content_trigger(
                        &id, &content, None,
                    );
                    center.addNotificationRequest_withCompletionHandler(&request, None);
                }
            }
            None => {
                for ev in events {
                    Self::osascript_notify(&ev.title, &ev.body);
                }
            }
        }
    }

    fn osascript_notify(title: &str, body: &str) {
        let script = format!(
            "display notification \"{}\" with title \"{}\" sound name \"default\"",
            Self::escape(body),
            Self::escape(title)
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

    fn escape(value: &str) -> String {
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', " ")
    }
}
