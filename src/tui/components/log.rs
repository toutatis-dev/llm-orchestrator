use super::Component;
use ratatui::{layout::Rect, Frame};

pub struct LogPanel;

impl LogPanel {
    pub fn new() -> Self {
        Self
    }
}

impl Component for LogPanel {
    fn render(&self, _frame: &mut Frame, _area: Rect) {
        // TODO: Implement log panel rendering
    }
}
