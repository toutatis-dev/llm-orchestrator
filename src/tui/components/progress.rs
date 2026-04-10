use super::Component;
use ratatui::{layout::Rect, Frame};

pub struct ProgressPanel;

impl ProgressPanel {
    pub fn new() -> Self {
        Self
    }
}

impl Component for ProgressPanel {
    fn render(&self, _frame: &mut Frame, _area: Rect) {
        // TODO: Implement progress panel rendering
    }
}
