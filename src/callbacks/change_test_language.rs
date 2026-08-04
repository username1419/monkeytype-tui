use std::{ffi::OsString, hash::Hash, sync::Arc};

use tokio::sync::Mutex;

use crate::{
    State,
    command::Command,
    notify::{QuickNotify, debug, enotify, error, notify},
    typing_test::word_list,
};

/// Creates the "Change language" command.
pub(crate) fn create() -> Command {
    Command::new(
        "changeLanguage".into(),
        "Change language".into(),
        crate::command::CommandGroup::Test,
        async move || Ok(true),
        Some(0),
        async move |s: Arc<Mutex<State>>| {
            s.lock().await.commandline.reset();

            let choices = match word_list::get_word_lists().await {
                Ok(choices) => choices,
                Err(e) => {
                    return Err(error!(e).to_string());
                }
            }
            .into_iter()
            .map(|language| {
                let language_string = language
                    .to_str()
                    .unwrap_or_else(|| panic!("language {:?} is not valid unicode", language))
                    .to_string();
                Command::new(
                    format!("changeLanguage_{}", language_string),
                    format!("Change language: {}", language_string),
                    crate::command::CommandGroup::Test,
                    async move || Ok(true),
                    None,
                    move |_| {
                        let language = language.clone();
                        async move {
                            word_list::update_and_get_words(language)
                                .await
                                .map(|_| ())
                                .map_err(|e| e.to_string())
                        }
                    },
                )
            })
            .collect();

            s.lock().await.commandline.prompt_command(
                "Select language...".into(),
                choices,
                move |_input, _command_opt| (),
            );

            Ok(())
        },
    )
}
