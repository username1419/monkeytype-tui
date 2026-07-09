use std::fmt::Debug;

use ratatui::{
    layout::{Position, Rect},
    style::{Style, Stylize},
    widgets::{Block, Paragraph, Wrap},
};
use tokio::spawn;

use crate::{
    command::{ClonedCommand, Command, Fuzzy, ROOT_COMMANDS},
    notify::{NOTIFICATIONS, QuickNotify},
    traits::UpdateableWidget,
};

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
    pub(crate) fn enable(&mut self) {
        self.enabled = true;
    }

    pub(crate) fn disable(&mut self) {
        self.enabled = false;
        if !self.is_searching() {
            self.reset();
        }
    }

    pub(crate) fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub(crate) fn is_searching(&self) -> bool {
        self.search
    }

    pub(crate) fn toggle_searching(&mut self) {
        self.search = !self.search;
    }

    pub(crate) fn register_select_up(&mut self) {
        let Some(s) = self.selected_command else {
            return;
        };
        let s = s.saturating_add(1);

        if self.matched_commands.len() > s {
            self.selected_command = Some(s);
        }
    }

    pub(crate) fn register_select_down(&mut self) {
        let Some(s) = self.selected_command else {
            return;
        };
        let s = s.saturating_sub(1);

        if self.matched_commands.len() > s {
            self.selected_command = Some(s);
        }
    }

    pub(crate) fn register_move_left(&mut self) {
        self.cursor_offset = self.cursor_offset.saturating_add(1);
    }

    pub(crate) fn register_move_right(&mut self) {
        self.cursor_offset = self.cursor_offset.saturating_sub(1);
    }

    pub(crate) fn register_character(&mut self, character: char) {
        self.input
            .insert(self.input.len() - self.cursor_offset as usize, character);
    }

    pub(crate) fn register_delete_character(&mut self) {
        if self.input.is_empty() {
            self.disable();
            return;
        }

        self.input
            .remove(self.input.len() - self.cursor_offset as usize - 1_usize);
    }

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

    fn get_input(&self) -> &String {
        &self.input
    }

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
            callback.0(self.input.clone(), cloned_command);
        }

        callback(self.input.clone(), command);

        if !remain_enabled {
            self.disable();
        }
    }

    pub(crate) fn reset(&mut self) {
        self.input.clear();
        self.cursor_offset = 0;
        self.matched_commands.clear();
        self.commands.clear();
        self.submit_callback = None;
        self.prompt = "Search...".into();
        self.search = true;
    }

    #[allow(private_bounds)]
    pub(crate) fn prompt_input(&mut self, prompt: String, callback: impl Into<SubmitCallback>) {
        self.reset();
        self.prompt = prompt;
        self.search = false;
        self.enable();

        self.submit_callback = Some(callback.into());
    }

    pub(crate) fn set_selectable_commands(&mut self, v: Vec<Command>) {
        self.commands = v;
    }
}

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

pub(crate) const COMMANDLINE_WIDTH: u16 = 60;
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
        for idx in self.matched_commands.iter() {
            let command = match self.root_command.is_none() {
                false => self.commands.get(*idx).unwrap(),
                true => c.get(*idx).unwrap(),
            };

            let mut widget = Paragraph::new(command.get_display_name().as_str())
                .white()
                .wrap(Wrap { trim: true });
            if let Some(sel_cmd) = self.selected_command
                && sel_cmd.eq(idx)
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

    fn update(&mut self) {
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

        let filter = c.find_fuzzy(&self.input, MAX_OPTIONS);

        self.matched_commands = filter;
        if self.selected_command.is_none() && !self.matched_commands.is_empty() {
            self.selected_command = Some(0);
        }
    }
}
