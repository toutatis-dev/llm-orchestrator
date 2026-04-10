pub mod chat;
pub mod log;
pub mod plan;
pub mod progress;
pub mod wizard;

pub use chat::ChatPanel;

use ratatui::Frame;

pub trait Component {
    fn render(&self, frame: &mut Frame, area: ratatui::layout::Rect);
}
