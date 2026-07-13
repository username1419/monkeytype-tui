use ratatui::Frame;

pub(crate) trait UpdateableWidget {
    fn render(&self, frame: &mut Frame, frame_width: u16, frame_height: u16);
    async fn update(&mut self);
}
