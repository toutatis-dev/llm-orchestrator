use ratatui::layout::{Alignment, Constraint, Direction, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, Tabs, Wrap};

pub fn default_layout(area: Rect) -> [Rect; 3] {
    let main_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Header
            Constraint::Min(0),    // Main content
            Constraint::Length(1), // Footer
        ])
        .split(area);

    let content_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(70), // Main panel
            Constraint::Percentage(30), // Info panel
        ])
        .split(main_layout[1]);

    [main_layout[0], content_layout[0], content_layout[1]]
}
