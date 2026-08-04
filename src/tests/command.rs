use std::sync::Arc;

use tokio::sync::Mutex;

use crate::{
    State,
    command::{Command, CommandGroup},
};

fn sample_command() -> Command {
    Command::new(
        "test.mode".into(),
        "test mode".into(),
        CommandGroup::Test,
        || async { Ok(true) },
        Some("words".into()),
        |_: Arc<Mutex<State>>| async { Ok(()) },
    )
}

#[test]
fn command_new_and_getters() {
    let command = sample_command();

    assert_eq!(command.get_id(), "test.mode");
    assert_eq!(command.get_display_name(), "test mode");
    assert!(matches!(command.get_group(), CommandGroup::Test));
    assert_eq!(command.get_selected_option(), Some(&"words".to_string()));
}

#[test]
fn command_clone_can_be_called() {
    let command = sample_command();
    let _cloned = command.clone();
}

#[tokio::test]
async fn command_call_invokes_handler() {
    let mut command = Command::new(
        "echo".into(),
        "echo".into(),
        CommandGroup::Other,
        || async { Ok(true) },
        None,
        |_: Arc<Mutex<State>>| async { Ok(()) },
    );

    let state = Arc::new(Mutex::new(State::default()));
    let handle = command.call(state);
    let result = handle.await.unwrap();

    assert_eq!(result, Ok(()));
}

#[test]
fn command_group_default_is_other() {
    assert!(matches!(CommandGroup::default(), CommandGroup::Other));
}
