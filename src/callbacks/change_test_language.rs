use std::sync::Arc;

use tokio::sync::Mutex;

use crate::{State, command::Command, typing_test::word_list};

/// Creates the "Change language" command.
pub(crate) fn create() -> Command {
    Command::new(
        "changeLanguage".into(),
        "Change language".into(),
        crate::command::CommandGroup::Test,
        async move || Ok(true),
        None,
        async move |s: Arc<Mutex<State>>| {
            s.lock().await.commandline.reset();

            Ok(())
        },
    )
}
