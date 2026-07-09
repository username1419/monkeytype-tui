use crate::command::Command;

pub(crate) mod login;

pub(crate) fn initialize_all() -> Vec<Command> {
    vec![login::create()]
}
