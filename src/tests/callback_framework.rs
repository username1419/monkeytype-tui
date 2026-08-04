//! Reusable framework for testing command handlers (TESTING.md "Callback framework").
//!
//! Covers:
//! - constructing a `Command` and invoking its handler via `Command::call(state).await`
//! - driving the one-shot prompt flow from the test side so handler input can be simulated
//! - injecting a fake `AUTHORIZATION` so commands can be tested without a real session
//!
//! Notification-side-effect assertions are intentionally absent: the notification
//! queue is private and no test observer exists yet.

use std::sync::Arc;

use tokio::sync::{Mutex, oneshot};

use crate::State;
use crate::auth::AUTHORIZATION;
use crate::command::{Command, CommandGroup};

/// Waits until the command line is in prompt mode (a handler has installed its
/// one-shot prompt), then types `input` and submits it.
async fn drive_prompt_input(state: &Arc<Mutex<State>>, input: &str) {
    loop {
        let guard = state.lock().await;
        if guard.commandline.is_enabled() && !guard.commandline.is_searching() {
            break;
        }
        drop(guard);
        tokio::task::yield_now().await;
    }

    let mut guard = state.lock().await;
    for c in input.chars() {
        guard.commandline.register_character(c);
    }
    guard.commandline.submit(false, |_, _| {});
}

#[tokio::test]
async fn command_handler_prompt_flow_returns_typed_input() {
    let state = Arc::new(Mutex::new(State::default()));
    let received = Arc::new(std::sync::Mutex::new(None::<String>));
    let sink = received.clone();
    let command = Command::new(
        "echo".into(),
        "echo".into(),
        CommandGroup::Other,
        || async { Ok(true) },
        None,
        move |state: Arc<Mutex<State>>| {
            let sink = sink.clone();
            async move {
                let (tx, rx) = oneshot::channel();
                state.lock().await.commandline.prompt_input(
                    "Enter name".into(),
                    move |input, _| {
                        tx.send(input).ok();
                    },
                );
                let input = rx.await.map_err(|_| "prompt dropped".to_string())?;
                *sink.lock().unwrap() = Some(input);
                Ok(())
            }
        },
    );
    let handle = command.call(state.clone());

    drive_prompt_input(&state, "alice").await;

    handle.await.unwrap().unwrap();
    assert_eq!(received.lock().unwrap().as_deref(), Some("alice"));
}

#[tokio::test]
async fn command_handler_prompt_flow_rejects_empty_input() {
    let state = Arc::new(Mutex::new(State::default()));
    let command = Command::new(
        "required".into(),
        "required".into(),
        CommandGroup::Other,
        || async { Ok(true) },
        None,
        |state: Arc<Mutex<State>>| async move {
            let (tx, rx) = oneshot::channel();
            state
                .lock()
                .await
                .commandline
                .prompt_input("Enter name".into(), move |input, _| {
                    tx.send(input).ok();
                });
            let input = rx.await.map_err(|_| "prompt dropped".to_string())?;
            if input.is_empty() {
                return Err("input is empty".to_string());
            }
            Ok(())
        },
    );
    let handle = command.call(state.clone());

    drive_prompt_input(&state, "").await;

    let result = handle.await.unwrap();
    assert_eq!(result, Err("input is empty".to_string()));
}

#[tokio::test]
async fn command_handler_can_read_injected_fake_authorization() {
    use tokio::time::Instant;

    use crate::auth::Authorization;

    let command = Command::new(
        "auth-check".into(),
        "auth check".into(),
        CommandGroup::Other,
        || async { Ok(true) },
        None,
        |_state: Arc<Mutex<State>>| async move {
            let auth = AUTHORIZATION.lock().expect("AUTHORIZATION is poisoned");
            if auth.is_logged_in() {
                Ok(())
            } else {
                Err("not logged in".to_string())
            }
        },
    );
    let state = Arc::new(Mutex::new(State::default()));

    {
        let mut guard = AUTHORIZATION.lock().unwrap();
        *guard = Authorization::new(
            "api".into(),
            "TestUser".into(),
            "tok".into(),
            Some(Instant::now()),
            3600,
            "Bearer".into(),
            "refresh".into(),
            "uid".into(),
            "proj".into(),
        );
    }

    let result = command.call(state).await.unwrap();
    assert_eq!(result, Ok(()));

    // Restore the default so other tests see a clean state.
    let mut guard = AUTHORIZATION.lock().unwrap();
    *guard = Authorization::default();
}
