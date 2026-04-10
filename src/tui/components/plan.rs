use super::Component;
use ratatui::{layout::Rect, Frame};

pub struct PlanPanel;

impl PlanPanel {
    pub fn new() -> Self {
        Self
    }
}

impl Component for PlanPanel {
    fn render(&self, _frame: &mut Frame, _area: Rect) {
        // TODO: Implement plan panel rendering
    }
}
