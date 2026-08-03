use std::time::Duration;

use crate::notify::{Notification, NotifLevel, debug, enotify, error, todo as notify_todo, wnotify};

#[test]
fn notif_level_display_labels() {
    assert_eq!(format!("{}", NotifLevel::Info), "INFO");
    assert_eq!(format!("{}", NotifLevel::Success), "SUCCESS");
    assert_eq!(format!("{}", NotifLevel::Warning), "WARNING");
    assert_eq!(format!("{}", NotifLevel::Debug), "DEBUG");
    assert_eq!(format!("{}", NotifLevel::Error), "ERROR");
}

#[test]
fn notification_display_shows_level_and_message() {
    let notification = Notification::builder()
        .title("Hello")
        .message("world")
        .notification_level(NotifLevel::Success)
        .build();

    assert_eq!(format!("{}", notification), "SUCCESS: world");
}

#[test]
fn builder_defaults_are_empty_message_and_info_level() {
    // Title and duration are not observable through the public API; only the
    // Display output (level + message) is reachable from tests.
    let notification = Notification::builder().build();

    assert_eq!(format!("{}", notification), "INFO: ");
}

#[test]
fn builder_fluent_setters_are_applied() {
    let notification = Notification::builder()
        .title("Update")
        .message("assets synced")
        .duration(Duration::from_secs(3))
        .notification_level(NotifLevel::Warning)
        .build();

    assert_eq!(format!("{}", notification), "WARNING: assets synced");
}

#[test]
fn debug_macro_fires_and_returns_value() {
    let value = debug!("hello world");
    assert_eq!(value, "hello world");
}

#[test]
fn error_macro_fires_and_returns_value() {
    let value = error!(42);
    assert_eq!(value, 42);
}

#[test]
fn enotify_wnotify_and_todo_macros_fire_without_panicking() {
    enotify!("boom");
    wnotify!("caution");
    notify_todo!();
}
