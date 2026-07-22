use once_cell::sync::Lazy;
use tokio::{spawn, sync::Mutex, task::JoinHandle};

use crate::{State, callbacks::initialize_all};
use std::{fmt::Debug, pin::Pin, sync::Arc};

pub(crate) static ROOT_COMMANDS: Lazy<Arc<[Command]>> = Lazy::new(|| initialize_all().into());

#[derive(Debug)]
pub(crate) struct Command {
    /// Internal name for the command
    id: String,
    /// Command name shown to user
    display_name: String,
    /// The category the command belongs to, to filter based on user needs
    group: CommandGroup,
    /// The condition in which the command is shown
    display_condition: Condition,
    /// Sub-options for the command
    options: Vec<String>,
    /// The selected sub-option
    selected_option: Option<String>,
    /// Callback which executes when the command is selected
    handler: CommandCallback,
}

#[derive(Default, Debug, Clone)]
pub(crate) enum CommandGroup {
    Test,
    Behavior,
    Input,
    Sound,
    Caret,
    Appearance,
    Theme,
    HideElements,
    Ads,
    #[default]
    Other,
}

impl Command {
    pub(crate) fn new(
        id: String,
        display_name: String,
        group: CommandGroup,
        display_condition: impl Into<Condition>,
        options: Vec<String>,
        selected_option: Option<String>,
        handler: impl Into<CommandCallback>,
    ) -> Self {
        Self {
            id,
            display_name,
            group,
            display_condition: display_condition.into(),
            options,
            selected_option,
            handler: handler.into(),
        }
    }

    pub fn get_id(&self) -> &String {
        &self.id
    }

    pub fn get_display_name(&self) -> &String {
        &self.display_name
    }

    pub fn get_group(&self) -> &CommandGroup {
        &self.group
    }

    pub fn get_options(&self) -> &Vec<String> {
        &self.options
    }

    pub fn get_selected_option(&self) -> Option<&String> {
        self.selected_option.as_ref()
    }

    pub fn call(&self, state: Arc<Mutex<State>>) -> JoinHandle<Result<(), String>> {
        spawn(self.handler.inner.as_ref()(state))
    }

    pub(crate) fn clone(&self) -> ClonedCommand {
        ClonedCommand {
            id: self.id.clone(),
            display_name: self.display_name.clone(),
            group: self.group.clone(),
            options: self.options.clone(),
            selected_option: self.selected_option.clone(),
        }
    }
}

#[derive(Debug, Default, Clone)]
pub(crate) struct ClonedCommand {
    id: String,
    display_name: String,
    group: CommandGroup,
    options: Vec<String>,
    selected_option: Option<String>,
}

pub(crate) trait Fuzzy {
    async fn find_fuzzy(&self, prompt: &str, options: u8) -> Vec<usize>;
}

impl Fuzzy for [Command] {
    async fn find_fuzzy(&self, prompt: &str, options: u8) -> Vec<usize> {
        let binding = prompt.to_ascii_lowercase();
        let terms = binding.split(char::is_whitespace).collect::<Vec<_>>();
        let mut v = Vec::with_capacity(options as usize);
        for (idx, command) in self.iter().enumerate() {
            let display_name = command.display_name.to_lowercase();
            let terms_command = display_name.split(char::is_whitespace);
            let match_strength = terms_command.zip(terms.iter()).fold(
                0,
                |mut match_strength, (term_command, term_prompt)| {
                    if term_command.starts_with(term_prompt) {
                        match_strength += term_prompt.len() as u16;
                    }
                    match_strength
                },
            );

            if match_strength == 0 {
                continue;
            }

            // NOTE: this is potentially more runtime expensive than the above check so we run it later
            // i think
            let display = command.display_condition.inner.as_ref()().await;
            if display.is_err() || display.is_ok_and(|display| !display) {
                continue;
            }

            if v.len() < options as usize {
                v.push((match_strength, idx));
                continue;
            }

            for (stored_strength, stored_idx) in v.iter_mut() {
                if match_strength >= *stored_strength {
                    *stored_strength = match_strength;
                    *stored_idx = idx;
                    break;
                }
            }
        }

        v.sort_by(|a, b| b.0.cmp(&a.0));
        v.into_iter().map(|(_, idx)| idx).collect()
    }
}

pub struct CommandCallback {
    #[allow(clippy::complexity)]
    pub(super) inner: Box<
        dyn Fn(Arc<Mutex<State>>) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>>
            + Sync
            + Send,
    >,
}

impl<F, Fut> From<F> for CommandCallback
where
    F: Fn(Arc<Mutex<State>>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<(), String>> + Send + 'static,
{
    fn from(inner: F) -> Self {
        Self {
            inner: Box::new(move |state| {
                Box::pin(inner(state)) as Pin<Box<dyn Future<Output = Result<(), String>> + Send>>
            }),
        }
    }
}

impl Debug for CommandCallback {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("callback")
    }
}

pub struct Condition {
    #[allow(clippy::complexity)]
    pub(super) inner:
        Box<dyn Fn() -> Pin<Box<dyn Future<Output = Result<bool, String>> + Send>> + Sync + Send>,
}

impl<F, Fut> From<F> for Condition
where
    F: Fn() -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<bool, String>> + Send + 'static,
{
    fn from(inner: F) -> Self {
        Self {
            inner: Box::new(move || {
                Box::pin(inner()) as Pin<Box<dyn Future<Output = Result<bool, String>> + Send>>
            }),
        }
    }
}

impl Debug for Condition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("condition")
    }
}
