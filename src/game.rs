use std::{error::Error, sync::Arc};
use tokio::sync::Mutex;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::{State, notify::enotify, test::TEST};

/// Handles a single key event, routing it to the command line or typing test
/// depending on whether the command line is enabled.
///
/// Keybindings:
/// - `Ctrl+Q` — initiate shutdown
/// - `Esc` — toggle command line
/// - `Enter` — submit command line input
/// - Printable characters — forwarded to the active input target
pub(crate) async fn event_keypressed(
    key: KeyEvent,
    state: Arc<Mutex<State>>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    match (key.modifiers, key.code) {
        (KeyModifiers::CONTROL, KeyCode::Char('q')) => {
            state.lock().await.shutdown.cancel();
        }
        (KeyModifiers::CONTROL, KeyCode::F(10)) => {
            enotify!("testing");
        }
        (KeyModifiers::NONE, KeyCode::Esc) => {
            let mut state = state.lock().await;
            let commandline = &mut state.commandline;
            match commandline.is_enabled() {
                true => commandline.disable(),
                false => commandline.enable(),
            }
        }
        (KeyModifiers::NONE, KeyCode::Char(c)) => {
            let mut state = state.lock().await;
            match state.commandline.is_enabled() {
                true => state.commandline.register_character(c),
                false => TEST.lock().expect("TEST is poisoned").register_character(c),
            }
        }
        (KeyModifiers::SHIFT, KeyCode::Char(c)) => {
            let mut state = state.lock().await;
            match state.commandline.is_enabled() {
                true => state.commandline.register_character(c.to_ascii_uppercase()),
                false => TEST
                    .lock()
                    .expect("TEST is poisoned")
                    .register_character(c.to_ascii_uppercase()),
            }
        }
        (KeyModifiers::NONE, KeyCode::Backspace) => {
            let mut state = state.lock().await;
            match state.commandline.is_enabled() {
                true => state.commandline.register_delete_character(),
                false => TEST
                    .lock()
                    .expect("TEST is poisoned")
                    .register_delete_character(),
            }
        }
        (KeyModifiers::CONTROL, KeyCode::Char('h'))
        | (KeyModifiers::CONTROL, KeyCode::Backspace) => {
            let mut state = state.lock().await;
            match state.commandline.is_enabled() {
                true => state.commandline.register_delete_word(),
                false => TEST
                    .lock()
                    .expect("TEST is poisoned")
                    .register_delete_word(),
            }
        }
        (KeyModifiers::NONE, KeyCode::Left) => {
            let mut state = state.lock().await;
            if state.commandline.is_enabled() {
                state.commandline.register_move_left()
            }
        }
        (KeyModifiers::NONE, KeyCode::Right) => {
            let mut state = state.lock().await;
            if state.commandline.is_enabled() {
                state.commandline.register_move_right()
            }
        }
        (KeyModifiers::NONE, KeyCode::Up) => {
            let mut state = state.lock().await;
            if state.commandline.is_enabled() {
                state.commandline.register_select_up()
            }
        }
        (KeyModifiers::NONE, KeyCode::Down) => {
            let mut state = state.lock().await;
            if state.commandline.is_enabled() {
                state.commandline.register_select_down()
            }
        }
        (KeyModifiers::NONE, KeyCode::Enter) => {
            let mut _state = state.lock().await;
            if _state.commandline.is_enabled() {
                let s = state.clone();
                _state.commandline.submit(false, |_input, command| {
                    if let Some(command) = command {
                        command.call(s);
                    }
                });
            }
        }
        (_mods, _code) => {}
    }

    Ok(())
}
