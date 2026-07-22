use ratatui::Frame;

/// Trait for components that can render themselves and update their internal state each frame.
pub(crate) trait UpdateableWidget {
    /// Draws the widget into the given ratatui frame.
    fn render(&self, frame: &mut Frame, frame_width: u16, frame_height: u16);
    /// Drives any time-based or event-driven internal state changes.
    async fn update(&mut self);
}
