use once_cell::sync::Lazy;
use tokio::{spawn, sync::Mutex, task::JoinHandle};

use crate::{State, callbacks::initialize_all};
use std::{fmt::Debug, pin::Pin, sync::Arc};

/// All registered commands, lazily initialized from [`callbacks::initialize_all`].
pub(crate) static ROOT_COMMANDS: Lazy<Arc<[Command]>> = Lazy::new(|| initialize_all().into());

/// A user-facing command with a display name, options, visibility condition, and async handler.
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
    /// The selected sub-option
    selected_option: Option<usize>,
    /// Callback which executes when the command is selected
    handler: CommandCallback,
}

/// Logical category for a [`Command`], used to filter commands in the UI.
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
    /// Creates a new command with the given metadata, visibility condition, and async handler.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        id: String,
        display_name: String,
        group: CommandGroup,
        display_condition: impl Into<Condition>,
        selected_option: Option<usize>,
        handler: impl Into<CommandCallback>,
    ) -> Self {
        Self {
            id,
            display_name,
            group,
            display_condition: display_condition.into(),
            selected_option,
            handler: handler.into(),
        }
    }

    /// Returns the internal command identifier.
    pub fn get_id(&self) -> &String {
        &self.id
    }

    /// Returns the command name shown to the user.
    pub fn get_display_name(&self) -> &String {
        &self.display_name
    }

    /// Returns the command's category group.
    pub fn get_group(&self) -> &CommandGroup {
        &self.group
    }

    /// Returns the currently selected sub-option, if any.
    pub fn get_selected_option(&self) -> Option<usize> {
        self.selected_option
    }

    /// Spawns the command's async handler with the given application state.
    pub fn call(&self, state: Arc<Mutex<State>>) -> JoinHandle<Result<(), String>> {
        spawn(self.handler.inner.as_ref()(state))
    }

    /// Creates a lightweight clone of this command without the handler closure.
    pub(crate) fn clone(&self) -> ClonedCommand {
        ClonedCommand {
            id: self.id.clone(),
            display_name: self.display_name.clone(),
            group: self.group.clone(),
            selected_option: self.selected_option,
        }
    }
}

/// A cloneable snapshot of a [`Command`] that omits the handler closure.
#[derive(Debug, Default, Clone)]
pub(crate) struct ClonedCommand {
    id: String,
    display_name: String,
    group: CommandGroup,
    selected_option: Option<usize>,
}

/// Fuzzy-match a list of commands against a user prompt.
pub(crate) trait Fuzzy {
    /// Returns the indices of the top `options` matching commands, sorted by match strength.
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

            if let Some((stored_strength, stored_idx)) = v.iter_mut().min()
                && match_strength >= *stored_strength
            {
                *stored_strength = match_strength;
                *stored_idx = idx;
                break;
            }
        }

        v.sort_by(|a, b| b.0.cmp(&a.0));
        v.into_iter().map(|(_, idx)| idx).collect()
    }
}

/// Type-erased async callback invoked when a [`Command`] is executed.
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

/// Type-erased async predicate that determines whether a [`Command`] should be shown.
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
