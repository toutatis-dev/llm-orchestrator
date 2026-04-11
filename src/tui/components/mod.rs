pub mod chat;
pub mod log;
pub mod plan;
pub mod progress;
pub mod wizard;

pub use chat::ChatPanel;
pub use plan::PlanPanel;
pub use wizard::{WizardAction, WizardPanel, WizardState};

use ratatui::Frame;

pub trait Component {
    fn render(&self, frame: &mut Frame, area: ratatui::layout::Rect);
}
