use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

/// Main layout structure
pub struct MainLayout {
    pub header: Rect,
    pub content: Rect,
    pub input: Rect,
    pub footer: Rect,
}

impl MainLayout {
    pub fn new(frame: &Frame) -> Self {
        let area = frame.area();

        let main_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // Header
                Constraint::Min(0),    // Content area
                Constraint::Length(3), // Input
                Constraint::Length(1), // Footer
            ])
            .split(area);

        Self {
            header: main_chunks[0],
            content: main_chunks[1],
            input: main_chunks[2],
            footer: main_chunks[3],
        }
    }

    pub fn render_header(&self, frame: &mut Frame, title: &str) {
        let header = Paragraph::new(title).style(Style::default().fg(Color::Cyan));
        frame.render_widget(header, self.header);
    }

    pub fn render_footer(&self, frame: &mut Frame, status: &str) {
        let footer = Paragraph::new(status).style(Style::default().fg(Color::Gray));
        frame.render_widget(footer, self.footer);
    }

    pub fn render_input(&self, frame: &mut Frame, content: &str, is_active: bool) {
        let border_style = if is_active {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        };

        let input = Paragraph::new(content).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border_style)
                .title("Input"),
        );
        frame.render_widget(input, self.input);
    }
}

/// Split content area into main and side panels
pub fn split_content(area: Rect, show_side_panel: bool) -> (Rect, Rect) {
    if show_side_panel {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
            .split(area);
        (chunks[0], chunks[1])
    } else {
        (area, Rect::default())
    }
}
