use std::sync::Arc;

use tokio::sync::Mutex;

use crate::{State, command::Command};

pub(crate) fn create() -> Command {
    // NOTE: good old currying
    Command::new(
        "changelanguage".into(),
        "Change language".into(),
        crate::command::CommandGroup::Test,
        async move || Ok(true),
        Vec::new(),
        None,
        async move |s: Arc<Mutex<State>>| {
            return Ok(());
        },
    )
}
