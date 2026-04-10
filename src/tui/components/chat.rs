use super::Component;
use ratatui::{layout::Rect, Frame};

pub struct ChatPanel;

impl ChatPanel {
    pub fn new() -> Self {
        Self
    }
}

impl Component for ChatPanel {
    fn render(&self, _frame: &mut Frame, _area: Rect) {
        // TODO: Implement chat panel rendering
    }
}
