use once_cell::sync::Lazy;
use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Style, Stylize},
    widgets::{Block, BorderType, Paragraph, Wrap},
};
use std::{
    cell::UnsafeCell,
    fmt::{Debug, Display},
    sync::{LazyLock, Mutex, OnceLock},
    time::{Duration, Instant},
};
use tokio::spawn;
use tokio::sync::{
    Semaphore,
    mpsc::{self, Receiver, Sender},
};

// NOTE: this notification system
// is truly a clusterfuck
// i should really refactor this

/// Wrapper around a tokio MPSC receiver that allows interior-mutable `try_recv`.
#[derive(Debug)]
struct MutableMpscReceiver(UnsafeCell<Receiver<Notification>>);
unsafe impl Sync for MutableMpscReceiver {}

impl MutableMpscReceiver {
    /// Non-blocking attempt to receive a pending notification.
    fn try_recv(&self) -> Option<Notification> {
        unsafe { (*self.0.get()).try_recv().ok() }
    }
}

/// Queue of active notifications awaiting display or expiry.
static NOTIFICATIONS: Lazy<Mutex<Vec<Notification>>> =
    Lazy::new(|| Mutex::new(Vec::with_capacity(12)));

/// Global MPSC receiver for incoming notifications (interior-mutable).
static NOTIFICATION_RECEIVER: LazyLock<MutableMpscReceiver> = LazyLock::new(|| {
    // NOTE: if we exceed this, were probably already fucked anyways
    let (tx, rx) = mpsc::channel(Semaphore::MAX_PERMITS);
    NOTIFICATION_SENDER
        .set(tx)
        .expect("channel already initialized");
    MutableMpscReceiver(UnsafeCell::new(rx))
});

/// Sender half of the notification channel.
static NOTIFICATION_SENDER: OnceLock<Sender<Notification>> = OnceLock::new();

/// Returns a raw mutable pointer to the global notification receiver.
fn receiver() -> *mut Receiver<Notification> {
    NOTIFICATION_RECEIVER.0.get()
}

/// Coordinates rendering and lifecycle of in-app notifications.
pub(crate) struct NotificationManager;

impl NotificationManager {
    /// Attempts to render all active notifications into the frame (non-blocking).
    pub(crate) fn try_render(frame: &mut ratatui::Frame, _frame_width: u16, _frame_height: u16) {
        if let Ok(notifications) = NOTIFICATIONS.try_lock() {
            render_notifications(&notifications, frame);
        }
    }

    /// Drains the notification channel and removes expired notifications.
    // NOTE: DO NOT CALL THIS TWICE, OR OUTSIDE OF UPDATE THREAD
    pub(crate) async fn update() {
        let mut notifications = NOTIFICATIONS.lock().unwrap();

        while let Ok(notification) = (unsafe { &mut *receiver() }).try_recv() {
            notifications.push(notification);
        }

        for idx in (0..notifications.len()).rev() {
            if notifications[idx].expires_at <= Instant::now() {
                notifications.remove(idx);
            }
        }
    }
}

/// Width of a single notification popup in columns.
const NOTIFICATION_WIDTH: u16 = 40;

/// Renders a slice of notifications as right-aligned popup blocks in the frame.
fn render_notifications(notifications: &[Notification], frame: &mut ratatui::Frame) {
    let area = frame.area();
    let mut running_length = 0_u16;

    for notification in notifications.iter().rev() {
        let color = match notification.level {
            NotifLevel::Success => Color::Green,
            NotifLevel::Error => Color::Red,
            NotifLevel::Warning | NotifLevel::Debug => Color::Yellow,
            NotifLevel::Info => Color::Cyan,
        };

        let height = f64::ceil(notification.message.len() as f64 / NOTIFICATION_WIDTH as f64)
            as u16
            + 2
            + notification
                .message
                .chars()
                .filter(char::is_ascii_control)
                .count() as u16;

        let popup_area = Rect {
            x: area.width.saturating_sub(NOTIFICATION_WIDTH),
            y: running_length,
            width: NOTIFICATION_WIDTH,
            height,
        };

        let paragraph = Paragraph::new(notification.message.as_str())
            .wrap(Wrap { trim: true })
            .white()
            .block(
                Block::bordered()
                    .title(format!("{}: {}", notification.level, notification.title))
                    .title_alignment(Alignment::Left)
                    .border_style(Style::default().fg(color))
                    .border_type(BorderType::Rounded),
            )
            .right_aligned();

        frame.render_widget(paragraph, popup_area);

        running_length += height;
    }
}

/// A single in-app notification with a title, message, severity level, and expiry time.
#[derive(Debug)]
pub(crate) struct Notification {
    title: String,
    message: String,
    expires_at: Instant,
    level: NotifLevel,
}

impl Notification {
    /// Creates a new notification directly (prefer [`Notification::builder`] instead).
    pub(super) fn new(
        title: String,
        message: String,
        expires_at: Instant,
        level: NotifLevel,
    ) -> Self {
        Self {
            title,
            message,
            expires_at,
            level,
        }
    }

    /// Returns a [`NotificationBuilder`] for constructing a notification with a fluent API.
    pub(crate) fn builder<'a>() -> NotificationBuilder<'a> {
        NotificationBuilder::new()
    }
}

impl Display for Notification {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{}: {}", self.level, self.message))
    }
}

/// Severity level of a [`Notification`], controlling its border color and display label.
#[derive(Debug)]
pub(crate) enum NotifLevel {
    Info,
    Success,
    Warning,
    Debug,
    Error,
}

impl Display for NotifLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            NotifLevel::Info => "INFO",
            NotifLevel::Success => "SUCCESS",
            NotifLevel::Warning => "WARNING",
            NotifLevel::Debug => "DEBUG",
            NotifLevel::Error => "ERROR",
        })
    }
}

/// Default display duration for quick notifications created via [`QuickNotify`].
pub(crate) const DEFAULT_QUICKNOTIFY_DURATION: Duration = Duration::from_secs(4);

/// Convenience trait for sending short-lived notifications with default duration.
pub(crate) trait QuickNotify {
    /// Sends an informational notification.
    fn info<T: Display>(&mut self, o: T);
    /// Sends a success notification.
    fn success(&mut self, s: &str);
}

/// Opaque handle returned by [`notify()`] for sending notifications.
pub(crate) struct Notify;

/// Returns a [`Notify`] handle for sending notifications.
pub(crate) fn notify() -> Notify {
    Notify
}

/// Sends a [`Notification`] through the global channel (or directly pushes when
/// the channel is not yet initialized).
pub(crate) fn send(notification: Notification) {
    #[cfg(not(test))]
    {
        match NOTIFICATION_SENDER.get() {
            Some(sender) => {
                let sender = sender.clone();
                spawn(async move {
                    let _ = sender.send(notification).await;
                });
            }
            None => {
                // NOTE: the only time this happens is right before the update thread is executed
                // for the first time

                if cfg!(debug_assertions) {
                    // if the render thread is fucked up somehow
                    println!("{}", notification);
                }
                NOTIFICATIONS.lock().unwrap().push(notification);
            }
        }
    }

    #[cfg(test)]
    {
        println!("{}", notification);
    }
}

impl QuickNotify for Notify {
    fn info<T: Display>(&mut self, o: T) {
        send(
            Notification::builder()
                .message(&format!("{o}"))
                .duration(DEFAULT_QUICKNOTIFY_DURATION)
                .notification_level(NotifLevel::Info)
                .build(),
        );
    }

    fn success(&mut self, s: &str) {
        send(
            Notification::builder()
                .message(s)
                .duration(DEFAULT_QUICKNOTIFY_DURATION)
                .notification_level(NotifLevel::Success)
                .build(),
        );
    }
}

/// Fluent builder for constructing [`Notification`] instances.
pub(crate) struct NotificationBuilder<'a> {
    title: &'a str,
    message: &'a str,
    duration: Duration,
    level: NotifLevel,
}

impl<'a> NotificationBuilder<'a> {
    /// Creates a new builder with empty defaults.
    pub(super) fn new() -> Self {
        Self {
            title: "",
            message: "",
            duration: Duration::ZERO,
            level: NotifLevel::Info,
        }
    }

    /// Sets the notification title displayed in the popup header.
    pub(crate) fn title(mut self, s: &'a str) -> Self {
        self.title = s;
        self
    }

    /// Sets the notification body text.
    pub(crate) fn message(mut self, s: &'a str) -> Self {
        self.message = s;
        self
    }

    /// Sets how long the notification remains visible before auto-dismissing.
    pub(crate) fn duration(mut self, d: Duration) -> Self {
        self.duration = d;
        self
    }

    /// Sets the severity level, which controls border color and label.
    pub(crate) fn notification_level(mut self, l: NotifLevel) -> Self {
        self.level = l;
        self
    }

    /// Consumes the builder and produces a [`Notification`].
    pub(crate) fn build(self) -> Notification {
        Notification::new(
            self.title.into(),
            self.message.into(),
            Instant::now() + self.duration,
            self.level,
        )
    }
}

/// Sends a debug-level notification showing the expression's file, line, name, and value.
macro_rules! debug {
    ($i:expr) => {
        match $i {
            tmp => {
                use crate::notify::*;
                send(
                    Notification::builder()
                        .message(&format!(
                            "[{}:{}:{}] {} = {:#?}",
                            std::file!(),
                            std::line!(),
                            std::column!(),
                            std::stringify!($i),
                            &tmp
                        ))
                        .duration(DEFAULT_QUICKNOTIFY_DURATION)
                        .notification_level(NotifLevel::Debug)
                        .build(),
                );
                tmp
            }
        }
    };
}
pub(crate) use debug;

/// Sends an error-level notification showing the expression's file, line, name, and value.
macro_rules! error {
    ($i:expr) => {
        match $i {
            tmp => {
                use crate::notify::*;
                send(
                    Notification::builder()
                        .message(&format!(
                            "[{}:{}:{}] {} = {:#?}",
                            std::file!(),
                            std::line!(),
                            std::column!(),
                            std::stringify!($i),
                            &tmp
                        ))
                        .duration(DEFAULT_QUICKNOTIFY_DURATION)
                        .notification_level(NotifLevel::Error)
                        .build(),
                );
                tmp
            }
        }
    };
}
pub(crate) use error;

/// Sends an error-level notification without returning the expression value.
macro_rules! enotify {
    ($i:expr) => {
        match &$i {
            tmp => {
                use crate::notify::*;
                send(
                    Notification::builder()
                        .message(&format!(
                            "[{}:{}:{}] {} = {:#?}",
                            std::file!(),
                            std::line!(),
                            std::column!(),
                            std::stringify!($i),
                            &tmp
                        ))
                        .duration(DEFAULT_QUICKNOTIFY_DURATION)
                        .notification_level(NotifLevel::Error)
                        .build(),
                );
            }
        }
    };
}
pub(crate) use enotify;

/// Sends a "not yet implemented" error notification with the call-site location.
macro_rules! todo {
    () => {
        use crate::notify::*;
        send(
            Notification::builder()
                .message(&format!(
                    "[{}:{}:{}] not yet implemented",
                    std::file!(),
                    std::line!(),
                    std::column!(),
                ))
                .duration(DEFAULT_QUICKNOTIFY_DURATION)
                .notification_level(NotifLevel::Error)
                .build(),
        );
    };
}
pub(crate) use todo;

/// Sends a warning-level notification with the expression's stringified value.
macro_rules! wnotify {
    ($i:expr) => {
        match $i {
            tmp => {
                use crate::notify::*;
                send(
                    Notification::builder()
                        .message(&tmp)
                        .duration(DEFAULT_QUICKNOTIFY_DURATION)
                        .notification_level(NotifLevel::Warning)
                        .build(),
                );
            }
        }
    };
}
pub(crate) use wnotify;
