use crate::core::{ChatMessage, Role};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
    Frame,
};

#[derive(Clone)]
pub struct ChatPanel {
    messages: Vec<ChatMessage>,
    scroll: usize,
    input: String,
    input_cursor: usize,
    focused: bool,
}

impl ChatPanel {
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            scroll: 0,
            input: String::new(),
            input_cursor: 0,
            focused: true,
        }
    }

    pub fn add_message(&mut self, message: ChatMessage) {
        self.messages.push(message);
        // Auto-scroll to bottom
        self.scroll = self.messages.len().saturating_sub(1);
    }

    pub fn scroll_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(1);
    }

    pub fn scroll_down(&mut self) {
        if self.scroll < self.messages.len().saturating_sub(1) {
            self.scroll += 1;
        }
    }

    pub fn input_char(&mut self, c: char) {
        self.input.insert(self.input_cursor, c);
        self.input_cursor += 1;
    }

    pub fn input_backspace(&mut self) {
        if self.input_cursor > 0 {
            self.input_cursor -= 1;
            self.input.remove(self.input_cursor);
        }
    }

    pub fn input_delete(&mut self) {
        if self.input_cursor < self.input.len() {
            self.input.remove(self.input_cursor);
        }
    }

    pub fn input_left(&mut self) {
        self.input_cursor = self.input_cursor.saturating_sub(1);
    }

    pub fn input_right(&mut self) {
        if self.input_cursor < self.input.len() {
            self.input_cursor += 1;
        }
    }

    pub fn clear_input(&mut self) -> String {
        let content = std::mem::take(&mut self.input);
        self.input_cursor = 0;
        content
    }

    pub fn get_input(&self) -> &str {
        &self.input
    }

    pub fn set_focus(&mut self, focused: bool) {
        self.focused = focused;
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(3)])
            .split(area);

        // Message area
        self.render_messages(frame, chunks[0]);

        // Input area
        self.render_input_box(frame, chunks[1]);
    }

    fn render_messages(&self, frame: &mut Frame, area: Rect) {
        let block = Block::default().borders(Borders::ALL).title("Chat");

        let inner_area = block.inner(area);

        // Convert messages to text lines
        let mut lines = Vec::new();
        for msg in &self.messages {
            let role_style = match msg.role {
                Role::User => Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
                Role::Orchestrator => Style::default().fg(Color::Blue),
                Role::System => Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::ITALIC),
            };

            // Role header
            lines.push(Line::from(Span::styled(
                format!("[{}]", msg.role),
                role_style,
            )));

            // Message content (split into lines)
            for line in msg.content.lines() {
                lines.push(Line::from(line.to_string()));
            }

            // Empty line between messages
            lines.push(Line::from(""));
        }

        let text = Text::from(lines);
        let paragraph = Paragraph::new(text)
            .block(block)
            .scroll((self.scroll as u16, 0));

        frame.render_widget(paragraph, area);

        // Render scrollbar
        let scrollbar = Scrollbar::default()
            .orientation(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("↑"))
            .end_symbol(Some("↓"));

        let mut scrollbar_state = ScrollbarState::new(self.messages.len()).position(self.scroll);

        frame.render_stateful_widget(scrollbar, inner_area, &mut scrollbar_state);
    }

    fn render_input_box(&self, frame: &mut Frame, area: Rect) {
        let border_style = if self.focused {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title("Input (Shift+Enter for newline)");

        let paragraph = Paragraph::new(self.input.clone()).block(block);

        frame.render_widget(paragraph, area);

        // Render cursor if focused
        if self.focused {
            // Calculate cursor position
            let cursor_x = area.x + 1 + self.input_cursor as u16;
            let cursor_y = area.y + 1;

            if cursor_x < area.x + area.width && cursor_y < area.y + area.height {
                frame.set_cursor_position((cursor_x, cursor_y));
            }
        }
    }
}

impl Default for ChatPanel {
    fn default() -> Self {
        Self::new()
    }
}
