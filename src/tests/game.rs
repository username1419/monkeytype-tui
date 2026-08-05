use std::sync::{Arc, Mutex};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tokio::sync::Mutex as AsyncMutex;

use crate::State;
use crate::commandline::CommandLine;
use crate::game::event_keypressed;

#[cfg(test)]
fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

/// Submits the command line and returns the input string passed to the callback.
#[cfg(test)]
fn capture_input(commandline: &mut CommandLine) -> String {
    if commandline.is_searching() {
        commandline.toggle_searching();
    }
    let captured = Arc::new(Mutex::new(String::new()));
    let c2 = captured.clone();
    commandline.submit(true, move |input, _| {
        *c2.lock().unwrap() = input;
    });
    captured.lock().unwrap().clone()
}

#[tokio::test]
async fn ctrl_q_cancels_shutdown() {
    let state = Arc::new(AsyncMutex::new(State::default()));
    event_keypressed(
        KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL),
        state.clone(),
    )
    .await
    .unwrap();

    assert!(state.lock().await.shutdown.is_cancelled());
}

#[tokio::test]
async fn esc_toggles_command_line_enabled_state() {
    let state = Arc::new(AsyncMutex::new(State::default()));
    assert!(!state.lock().await.commandline.is_enabled());

    event_keypressed(key(KeyCode::Esc), state.clone())
        .await
        .unwrap();
    assert!(state.lock().await.commandline.is_enabled());

    event_keypressed(key(KeyCode::Esc), state.clone())
        .await
        .unwrap();
    assert!(!state.lock().await.commandline.is_enabled());
}

#[tokio::test]
async fn character_routes_to_command_line_when_enabled() {
    let state = Arc::new(AsyncMutex::new(State::default()));
    state.lock().await.commandline.enable();

    event_keypressed(key(KeyCode::Char('h')), state.clone())
        .await
        .unwrap();
    event_keypressed(key(KeyCode::Char('i')), state.clone())
        .await
        .unwrap();

    let captured = {
        let mut guard = state.lock().await;
        capture_input(&mut guard.commandline)
    };
    assert_eq!(captured, "hi");
}

#[tokio::test]
async fn shift_character_routes_uppercased_character() {
    let state = Arc::new(AsyncMutex::new(State::default()));
    state.lock().await.commandline.enable();

    event_keypressed(
        KeyEvent::new(KeyCode::Char('a'), KeyModifiers::SHIFT),
        state.clone(),
    )
    .await
    .unwrap();

    let captured = {
        let mut guard = state.lock().await;
        capture_input(&mut guard.commandline)
    };
    assert_eq!(captured, "A");
}

#[tokio::test]
async fn enter_submits_and_does_not_disable_command_line_when_enabled() {
    let state = Arc::new(AsyncMutex::new(State::default()));
    state.lock().await.commandline.enable();
    event_keypressed(key(KeyCode::Enter), state.clone())
        .await
        .unwrap();

    assert!(state.lock().await.commandline.is_enabled());
}

#[tokio::test]
async fn enter_is_a_noop_when_command_line_disabled() {
    let state = Arc::new(AsyncMutex::new(State::default()));
    event_keypressed(key(KeyCode::Enter), state.clone())
        .await
        .unwrap();

    let guard = state.lock().await;
    assert!(!guard.commandline.is_enabled());
    assert!(!guard.shutdown.is_cancelled());
}

#[tokio::test]
async fn left_arrow_moves_cursor_in_command_line() {
    let state = Arc::new(AsyncMutex::new(State::default()));
    state.lock().await.commandline.enable();

    event_keypressed(key(KeyCode::Char('a')), state.clone())
        .await
        .unwrap();
    event_keypressed(key(KeyCode::Char('b')), state.clone())
        .await
        .unwrap();
    event_keypressed(key(KeyCode::Left), state.clone())
        .await
        .unwrap();
    event_keypressed(key(KeyCode::Char('x')), state.clone())
        .await
        .unwrap();

    let captured = {
        let mut guard = state.lock().await;
        capture_input(&mut guard.commandline)
    };
    assert_eq!(captured, "axb");
}

#[tokio::test]
async fn arrow_keys_are_noops_when_command_line_disabled() {
    let state = Arc::new(AsyncMutex::new(State::default()));

    for code in [KeyCode::Left, KeyCode::Right, KeyCode::Up, KeyCode::Down] {
        event_keypressed(key(code), state.clone()).await.unwrap();
    }

    let guard = state.lock().await;
    assert!(!guard.commandline.is_enabled());
    assert!(guard.commandline.is_searching());
}

#[tokio::test]
async fn ctrl_backspace_deletes_word_in_command_line() {
    let state = Arc::new(AsyncMutex::new(State::default()));
    state.lock().await.commandline.enable();

    for c in "foo bar".chars() {
        event_keypressed(key(KeyCode::Char(c)), state.clone())
            .await
            .unwrap();
    }
    event_keypressed(
        KeyEvent::new(KeyCode::Backspace, KeyModifiers::CONTROL),
        state.clone(),
    )
    .await
    .unwrap();

    let captured = {
        let mut guard = state.lock().await;
        capture_input(&mut guard.commandline)
    };
    assert_eq!(captured, "foo");
}

#[tokio::test]
async fn ctrl_h_deletes_word_in_command_line() {
    let state = Arc::new(AsyncMutex::new(State::default()));
    state.lock().await.commandline.enable();

    for c in "foo bar".chars() {
        event_keypressed(key(KeyCode::Char(c)), state.clone())
            .await
            .unwrap();
    }
    event_keypressed(
        KeyEvent::new(KeyCode::Char('h'), KeyModifiers::CONTROL),
        state.clone(),
    )
    .await
    .unwrap();

    let captured = {
        let mut guard = state.lock().await;
        capture_input(&mut guard.commandline)
    };
    assert_eq!(captured, "foo");
}
