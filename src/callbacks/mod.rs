use crate::command::Command;

pub(crate) mod change_test_language;
pub(crate) mod login;

pub(crate) fn initialize_all() -> Vec<Command> {
    vec![vec![login::create()], vec![change_test_language::create()]]
        .into_iter()
        .flatten()
        .collect()
}
