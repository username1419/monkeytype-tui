use std::sync::Arc;

use tokio::sync::Mutex;

use crate::{
    State,
    command::{Command, ROOT_COMMANDS},
    notify::{debug, error},
    typing_test::word_list,
};

const ID: &str = "changeLanguage";
/// Creates the "Change language" command.
///
/// When invoked, this handler resets the command line, loads every available word list, and
/// presents a sub-command per language via the command line's prompt mode. Selecting a
/// language calls [`word_list::update_and_get_words`] to switch the active test language.
pub(crate) fn create() -> Command {
    Command::new(
        ID.into(),
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

            let root = ROOT_COMMANDS
                .iter()
                .position(|c| ID.to_string().eq(c.get_id()))
                .unwrap_or_else(|| {
                    panic!(
                        "The command {} is not present in ROOT_COMMANDS, but is called",
                        ID
                    )
                });
            s.lock().await.commandline.prompt_command(
                "Select language...".into(),
                Some(root),
                choices,
                move |_input, _command_opt| (),
            );

            Ok(())
        },
    )
}
