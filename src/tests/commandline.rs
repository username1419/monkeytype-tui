use std::sync::{Arc, Mutex};

use crate::commandline::CommandLine;
use crate::notify::debug;
use crate::traits::UpdateableWidget;

/// Submits the command line and returns the input string passed to the callback.
///
/// There is no getter for the input, so all state observation goes through the
/// `submit` callback (see TESTING.md §1).
#[cfg(test)]
fn observe_input(cl: &mut CommandLine) -> String {
    if cl.is_searching() {
        cl.toggle_searching();
    }
    let captured = Arc::new(Mutex::new(String::new()));
    let c2 = captured.clone();
    cl.submit(true, move |input, _| {
        *c2.lock().unwrap() = input;
    });
    captured.lock().unwrap().clone()
}

#[test]
fn default_state_is_disabled_and_searching() {
    let cl = CommandLine::default();
    assert!(!cl.is_enabled());
    assert!(cl.is_searching());
}

#[test]
fn enable_and_disable_flip_enabled_state() {
    let mut cl = CommandLine::default();
    cl.enable();
    assert!(cl.is_enabled());
    cl.disable();
    assert!(!cl.is_enabled());
}

#[test]
fn disable_in_search_mode_does_not_reset() {
    let mut cl = CommandLine::default();
    cl.enable();
    cl.register_character('a');
    cl.register_character('b');
    cl.disable();

    assert!(!cl.is_enabled());
    assert!(cl.is_searching());
    assert_eq!(observe_input(&mut cl), "ab");
}

#[test]
fn disable_in_prompt_mode_resets_input() {
    let mut cl = CommandLine::default();
    cl.prompt_input("Enter name".into(), |_, _| {});
    assert!(!cl.is_searching());
    cl.register_character('a');
    cl.register_character('b');
    cl.disable();

    assert!(!cl.is_enabled());
    assert!(cl.is_searching());
    assert_eq!(observe_input(&mut cl), "");
}

#[test]
fn toggle_searching_flips_search_mode() {
    let mut cl = CommandLine::default();
    assert!(cl.is_searching());
    cl.toggle_searching();
    assert!(!cl.is_searching());
    cl.toggle_searching();
    assert!(cl.is_searching());
}

#[test]
fn characters_append_at_cursor_offset_zero() {
    let mut cl = CommandLine::default();
    cl.enable();
    cl.register_character('a');
    cl.register_character('b');
    cl.register_character('c');
    assert_eq!(observe_input(&mut cl), "abc");
}

#[test]
fn moving_cursor_left_changes_insertion_point() {
    let mut cl = CommandLine::default();
    cl.enable();
    cl.register_character('a');
    cl.register_character('b');
    cl.register_move_left();
    cl.register_character('x');
    assert_eq!(observe_input(&mut cl), "axb");
}

#[test]
fn moving_cursor_right_restores_insertion_point() {
    let mut cl = CommandLine::default();
    cl.enable();
    cl.register_character('a');
    cl.register_character('b');
    cl.register_move_left();
    cl.register_character('x');
    cl.register_move_right();
    cl.register_character('y');
    assert_eq!(observe_input(&mut cl), "axby");
}

#[test]
fn delete_character_removes_char_before_cursor() {
    let mut cl = CommandLine::default();
    cl.enable();
    cl.register_character('a');
    cl.register_character('b');
    cl.register_character('c');
    cl.register_delete_character();
    assert_eq!(observe_input(&mut cl), "ab");

    cl.register_character('c');
    cl.register_move_left();
    cl.register_delete_character();
    assert_eq!(observe_input(&mut cl), "ac");
}

#[test]
fn delete_character_on_empty_input_disables_command_line() {
    let mut cl = CommandLine::default();
    cl.enable();
    cl.register_delete_character();
    assert!(!cl.is_enabled());
}

#[test]
fn delete_word_removes_word_before_cursor() {
    let mut cl = CommandLine::default();
    cl.enable();
    for c in "foo bar baz".chars() {
        cl.register_character(c);
    }
    cl.register_delete_word();

    // "baz" (and the trailing space) is removed, leaving "foo bar" with the
    // cursor at the end (cursor_offset 0, so typing appends).
    assert_eq!(observe_input(&mut cl), "foo bar");

    let mut cl = CommandLine::default();
    cl.enable();
    for c in "foo bar baz".chars() {
        cl.register_character(c);
    }
    cl.register_delete_word();
    cl.register_character('x');
    assert_eq!(observe_input(&mut cl), "foo barx");
}

#[test]
fn prompt_input_switches_to_prompt_mode() {
    let mut cl = CommandLine::default();
    cl.prompt_input("Enter email...".into(), |_, _| {});
    assert!(cl.is_enabled());
    assert!(!cl.is_searching());
}

#[test]
fn submit_in_prompt_mode_invokes_oneshot_callback_with_input() {
    let mut cl = CommandLine::default();
    let captured = Arc::new(Mutex::new(None));
    let c2 = captured.clone();
    cl.prompt_input("Enter email...".into(), move |input, command| {
        *c2.lock().unwrap() = Some((input, command));
    });

    cl.register_character('u');
    cl.register_character('s');
    cl.register_character('e');
    cl.register_character('r');
    cl.submit(false, |_, _| {});

    let (input, command) = captured.lock().unwrap().take().unwrap();
    assert_eq!(input, "user");
    assert!(command.is_none());
    assert!(!cl.is_enabled());
}

#[test]
fn submit_with_remain_enabled_keeps_command_line_enabled() {
    let mut cl = CommandLine::default();
    cl.prompt_input("Enter name".into(), |_, _| {});
    cl.register_character('a');
    cl.submit(true, |_, _| {});
    assert!(cl.is_enabled());
}

#[tokio::test]
async fn search_mode_update_populates_matches_and_submit_returns_selected_command() {
    let mut cl = CommandLine::default();
    cl.enable();

    for c in "change".chars() {
        cl.register_character(c);
    }
    cl.update().await;

    let captured = Arc::new(Mutex::new(None));
    let c2 = captured.clone();
    cl.submit(false, move |input, command| {
        *c2.lock().unwrap() = Some((input, command.map(|c| c.get_id().clone())));
    });

    let (input, id) = captured.lock().unwrap().take().unwrap();
    assert_eq!(input, "change");
    assert_eq!(id.as_deref(), Some("changeLanguage"));
    assert!(!cl.is_enabled());
}

#[tokio::test]
async fn search_mode_submit_without_matches_disables_command_line() {
    let mut cl = CommandLine::default();
    cl.enable();
    cl.register_character('q');
    cl.register_character('q');
    cl.register_character('q');
    cl.update().await;

    let captured = Arc::new(Mutex::new(None));
    let c2 = captured.clone();
    cl.submit(false, move |input, command| {
        *c2.lock().unwrap() = Some((input, command.map(|c| c.get_id().clone())));
    });

    assert!(captured.lock().unwrap().is_none())
}
