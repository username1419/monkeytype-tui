use std::sync::{Arc, Mutex};

use once_cell::sync::Lazy;
use ratatui::{
    layout::Rect,
    widgets::{Paragraph, Wrap},
};
use tokio::time::Instant;

use crate::traits::UpdateableWidget;

/// A typing test session that tracks target words, user input, and display state.
#[derive(Default)]
pub(crate) struct Test {
    /// The word list which gets drawn. Tracks current_word_list
    display_word_list: String,
    /// The list of words which target_word_list is picked from
    words_list: Vec<String>,
    /// Typed words within this test period
    current_word_list: Vec<String>,
    /// The target which current_word_list aims towards
    target_word_list: Vec<String>,
    current_errors: Vec<(usize, usize)>,
    key_timestamp: Vec<(Instant, char)>,
}

impl Test {
    /// Appends a character to the current word, or advances to the next word
    /// if the current word already matches the target.
    pub(crate) fn register_character(&mut self, character: char) {
        if self.current_word_list.is_empty() {
            self.current_word_list.push(String::new());
        }

        if let Some(w) = self.current_word_list.last()
            && self.target_word_list[self.current_word_list.len() - 1].eq(w)
        {
            self.current_word_list.push(String::new());
            self.regenerate_word_display();
            return;
        }

        self.current_word_list.last_mut().unwrap().push(character);
        self.regenerate_word_display();
    }

    /// Removes the last character from the current word.
    pub(crate) fn register_delete_character(&mut self) {
        if self.current_word_list.is_empty() {
            return;
        }

        self.current_word_list.last_mut().unwrap().pop();
        self.regenerate_word_display();
    }

    /// Clears the entire current word (Ctrl+Backspace / Ctrl+H).
    pub(crate) fn register_delete_word(&mut self) {
        if self.current_word_list.is_empty() {
            return;
        }

        self.current_word_list.last_mut().unwrap().clear();
        self.regenerate_word_display();
    }

    /// Rebuilds the display string from the current word list.
    pub(crate) fn regenerate_word_display(&mut self) {
        self.display_word_list = self.current_word_list.join(" ");
    }

    /// Resets the test to its initial empty state.
    pub(crate) fn reset(&mut self) {
        self.words_list = Vec::new();
        self.display_word_list = String::new();
        self.current_word_list = Vec::new();
        self.target_word_list = Vec::new();
    }
}

impl UpdateableWidget for Test {
    fn render(&self, frame: &mut ratatui::Frame, frame_width: u16, frame_height: u16) {
        let width = (frame_width as f64 * 0.75) as u16;
        let height = 3;
        frame.render_widget(
            Paragraph::new(self.display_word_list.as_str()).wrap(Wrap { trim: false }),
            Rect::new(
                frame_width / 2 - width / 2,
                frame_height / 2 - height / 2,
                width,
                height,
            ),
        );
    }

    async fn update(&mut self) {}
}

/// Global typing test instance, shared across threads.
pub(crate) static TEST: Lazy<Arc<Mutex<Test>>> =
    Lazy::new(|| Arc::new(Mutex::new(Test::default())));
