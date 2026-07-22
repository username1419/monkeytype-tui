use std::fmt::Debug;

use crate::{
    command::{ClonedCommand, Command, Fuzzy, ROOT_COMMANDS},
    notify,
    traits::UpdateableWidget,
};
use ratatui::{
    layout::{Position, Rect},
    style::{Style, Stylize},
    widgets::{Block, Paragraph, Wrap},
};

/// An interactive command line with fuzzy search, text input, and cursor navigation.
#[derive(Debug)]
pub(crate) struct CommandLine {
    // TODO: make cancel callback
    enabled: bool,
    search: bool,
    prompt: String,
    commands: Vec<Command>,
    input: String,
    root_command: Option<usize>,
    /// NOTE: this is relative to self.matched_commands, NOT self.commands
    selected_command: Option<usize>,
    matched_commands: Vec<usize>,
    // ideally thered be a sliding window to show matched commands but idc
    /// NOTE: this offset is negative
    cursor_offset: u16,
    submit_callback: Option<SubmitCallback>,
}

/// One-shot callback invoked when the command line input is submitted.
struct SubmitCallback(pub Box<dyn FnOnce(String, Option<ClonedCommand>) + Send>);
impl Debug for SubmitCallback {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SubmitCallback")
    }
}

impl<F> From<F> for SubmitCallback
where
    // NOTE: i dont really like this but i forgor how to declare named lifetimes
    F: FnOnce(String, Option<ClonedCommand>) + Send + 'static,
{
    fn from(f: F) -> Self {
        Self(Box::new(f))
    }
}

impl CommandLine {
    /// Enables the command line, making it visible and ready for input.
    pub(crate) fn enable(&mut self) {
        self.enabled = true;
    }

    /// Disables the command line, hiding it and resetting state unless in search mode.
    pub(crate) fn disable(&mut self) {
        self.enabled = false;
        if !self.is_searching() {
            self.reset();
        }
    }

    /// Returns whether the command line is currently visible.
    pub(crate) fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Returns whether the command line is in fuzzy-search mode (as opposed to prompt mode).
    pub(crate) fn is_searching(&self) -> bool {
        self.search
    }

    /// Toggles between search mode and prompt mode.
    pub(crate) fn toggle_searching(&mut self) {
        self.search = !self.search;
    }

    /// Moves the selection cursor up through the matched commands list.
    pub(crate) fn register_select_up(&mut self) {
        let Some(s) = self.selected_command else {
            return;
        };
        let s = s.saturating_add(1);

        if self.matched_commands.len() > s {
            self.selected_command = Some(s);
        }
    }

    /// Moves the selection cursor down through the matched commands list.
    pub(crate) fn register_select_down(&mut self) {
        let Some(s) = self.selected_command else {
            return;
        };
        let s = s.saturating_sub(1);

        if self.matched_commands.len() > s {
            self.selected_command = Some(s);
        }
    }

    /// Shifts the text viewport left (increases cursor offset).
    pub(crate) fn register_move_left(&mut self) {
        self.cursor_offset = self.cursor_offset.saturating_add(1);
    }

    /// Shifts the text viewport right (decreases cursor offset).
    pub(crate) fn register_move_right(&mut self) {
        self.cursor_offset = self.cursor_offset.saturating_sub(1);
    }

    /// Inserts a character at the current cursor position.
    pub(crate) fn register_character(&mut self, character: char) {
        self.input
            .insert(self.input.len() - self.cursor_offset as usize, character);
    }

    /// Removes the character before the cursor; disables the command line if input is empty.
    pub(crate) fn register_delete_character(&mut self) {
        if self.input.is_empty() {
            self.disable();
            return;
        }

        self.input
            .remove(self.input.len() - self.cursor_offset as usize - 1_usize);
    }

    /// Deletes the word before the cursor (Ctrl+Backspace / Ctrl+H).
    pub(crate) fn register_delete_word(&mut self) {
        let range_start = self
            .input
            .chars()
            .rev()
            .zip((0..self.input.len()).rev())
            .skip(self.cursor_offset as usize)
            .find_map(|(ch, idx)| if ch.is_whitespace() { Some(idx) } else { None })
            .unwrap_or_default();
        let range_end = self.input.len() - self.cursor_offset as usize;
        self.input = self
            .input
            .chars()
            .enumerate()
            .filter_map(|(idx, ch)| {
                if idx >= range_start && idx < range_end {
                    None
                } else {
                    Some(ch)
                }
            })
            .collect();
        self.cursor_offset = self.input.len().saturating_sub(range_start) as u16;
    }

    /// Returns the current input string.
    fn get_input(&self) -> &String {
        &self.input
    }

    /// Submits the current input, invoking the callback with the input text
    /// and the currently selected command (if any).
    pub(crate) fn submit(
        &mut self,
        remain_enabled: bool,
        callback: impl FnOnce(String, Option<&Command>),
    ) {
        // this sucks but any other way would introduce errors
        let _c = ROOT_COMMANDS.clone();
        let command = if self.is_searching() {
            match self.root_command.is_none() {
                true => self
                    .selected_command
                    .map(|idx| &_c[self.matched_commands[idx]]),
                false => {
                    let c = &self.commands;
                    self.selected_command
                        .map(|idx| &c[self.matched_commands[idx]])
                }
            }
        } else {
            None
        };

        let cloned_command = command.iter().map(|c| (*c).clone()).next();
        if let Some(callback) = self.submit_callback.take() {
            callback.0(self.input.clone(), notify::debug!(cloned_command));
        }

        callback(self.input.clone(), command);

        if !remain_enabled {
            self.disable();
        }
    }

    /// Resets all command line state to defaults.
    pub(crate) fn reset(&mut self) {
        self.input.clear();
        self.cursor_offset = 0;
        self.matched_commands.clear();
        self.commands.clear();
        self.submit_callback = None;
        self.prompt = "Search...".into();
        self.search = true;
    }

    /// Switches to prompt mode with a custom prompt string and a one-shot submit callback.
    #[allow(private_bounds)]
    pub(crate) fn prompt_input(&mut self, prompt: String, callback: impl Into<SubmitCallback>) {
        self.reset();
        self.prompt = prompt;
        self.search = false;
        self.enable();

        self.submit_callback = Some(callback.into());
    }

    /// Replaces the list of commands available for fuzzy search.
    pub(crate) fn set_selectable_commands(&mut self, v: Vec<Command>) {
        self.commands = v;
    }
}

/// Maximum number of fuzzy-match results displayed at once.
const MAX_OPTIONS: u8 = 10;
impl Default for CommandLine {
    fn default() -> Self {
        Self {
            enabled: false,
            search: true,
            prompt: "Search...".into(),
            commands: Vec::default(),
            input: String::default(),
            root_command: None,
            selected_command: None,
            matched_commands: Vec::with_capacity(MAX_OPTIONS as usize),
            cursor_offset: 0,
            submit_callback: None,
        }
    }
}

/// Width of the command line widget in columns.
pub(crate) const COMMANDLINE_WIDTH: u16 = 60;
/// Vertical offset from center for the command line position.
pub(crate) const COMMANDLINE_Y_OFFSET: i16 = -10;
impl UpdateableWidget for CommandLine {
    fn render(&self, frame: &mut ratatui::Frame, frame_width: u16, frame_height: u16) {
        if !self.enabled {
            return;
        }

        let height = 2 // Input box borders
            + 1; // Input box contents

        let x = frame_width / 2 - COMMANDLINE_WIDTH / 2;
        let y = ((frame_height / 2) as i16 + COMMANDLINE_Y_OFFSET) as u16;

        frame.render_widget(
            Block::bordered()
                .border_type(ratatui::widgets::BorderType::Rounded)
                .bold(),
            Rect::new(x, y, COMMANDLINE_WIDTH, height),
        );

        let text_input_position = Position::new(x + 2, y + 1);
        // NOTE: placeholder
        if self.input.is_empty() {
            frame.render_widget(
                Paragraph::new(self.prompt.as_str()).light_blue(),
                Rect::new(
                    text_input_position.x,
                    text_input_position.y,
                    COMMANDLINE_WIDTH - 2,
                    1,
                ),
            );
        }

        frame.set_cursor_position((
            text_input_position.x + self.input.len() as u16 - self.cursor_offset,
            text_input_position.y,
        ));
        frame.render_widget(
            Paragraph::new(self.input.as_str()).white(),
            Rect::new(
                text_input_position.x,
                text_input_position.y,
                COMMANDLINE_WIDTH - 2,
                1,
            ),
        );

        if !self.is_searching() {
            return;
        }

        let c = ROOT_COMMANDS.clone();
        let len = self.matched_commands.len();
        for (m_idx, idx) in self.matched_commands.iter().enumerate() {
            let command = match self.root_command.is_none() {
                false => self.commands.get(*idx).unwrap(),
                true => c.get(*idx).unwrap(),
            };

            let mut widget = Paragraph::new(command.get_display_name().as_str())
                .white()
                .wrap(Wrap { trim: true });
            if let Some(sel_cmd) = self.selected_command
                && sel_cmd == m_idx
            {
                widget = widget.on_blue().black();
            }

            frame.render_widget(
                widget,
                Rect::new(
                    text_input_position.x,
                    text_input_position.y + 3,
                    COMMANDLINE_WIDTH - 4,
                    1,
                ),
            );
        }

        frame.render_widget(
            Block::bordered()
                .border_type(ratatui::widgets::BorderType::Rounded)
                .border_style(Style::new().fg(ratatui::style::Color::Blue)),
            Rect::new(x, y + 3, COMMANDLINE_WIDTH, len as u16 + 2),
        );
    }

    async fn update(&mut self) {
        if !self.enabled || !self.search {
            return;
        }

        if self.input.is_empty() {
            self.matched_commands.clear();
            self.selected_command = None;
            return;
        }

        let c = match self.root_command.is_none() {
            true => &*ROOT_COMMANDS.clone(),
            false => &self.commands,
        };

        let filter = c.find_fuzzy(&self.input, MAX_OPTIONS).await;

        self.matched_commands = filter;
        if self.selected_command.is_none() && !self.matched_commands.is_empty() {
            self.selected_command = Some(0);
        }
    }
}
