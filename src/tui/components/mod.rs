pub mod chat;
pub mod log;
pub mod plan;
pub mod progress;
pub mod wizard;

use ratatui::Frame;

pub trait Component {
    fn render(&self, frame: &mut Frame, area: ratatui::layout::Rect);
}