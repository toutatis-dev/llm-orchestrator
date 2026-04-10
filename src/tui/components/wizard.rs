use super::Component;
use ratatui::{layout::Rect, Frame};

pub struct WizardPanel;

impl WizardPanel {
    pub fn new() -> Self {
        Self
    }
}

impl Component for WizardPanel {
    fn render(&self, _frame: &mut Frame, _area: Rect) {
        // TODO: Implement wizard panel rendering
    }
}
