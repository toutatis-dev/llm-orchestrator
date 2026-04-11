use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

#[derive(Clone, Debug)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    fn color(&self) -> Color {
        match self {
            LogLevel::Debug => Color::Gray,
            LogLevel::Info => Color::Cyan,
            LogLevel::Warn => Color::Yellow,
            LogLevel::Error => Color::Red,
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            LogLevel::Debug => "DEBUG",
            LogLevel::Info => "INFO",
            LogLevel::Warn => "WARN",
            LogLevel::Error => "ERROR",
        }
    }
}

#[derive(Clone, Debug)]
pub struct LogEntry {
    pub timestamp: chrono::DateTime<chrono::Local>,
    pub level: LogLevel,
    pub message: String,
    pub source: Option<String>,
}

impl LogEntry {
    pub fn new(level: LogLevel, message: impl Into<String>) -> Self {
        Self {
            timestamp: chrono::Local::now(),
            level,
            message: message.into(),
            source: None,
        }
    }

    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }
}

pub struct LogPanel {
    entries: Vec<LogEntry>,
    scroll: usize,
    max_entries: usize,
}

impl LogPanel {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            scroll: 0,
            max_entries: 1000,
        }
    }

    pub fn with_max_entries(mut self, max: usize) -> Self {
        self.max_entries = max;
        self
    }

    pub fn add_entry(&mut self, entry: LogEntry) {
        self.entries.push(entry);
        // Trim old entries
        if self.entries.len() > self.max_entries {
            self.entries.remove(0);
        }
    }

    pub fn log(&mut self, level: LogLevel, message: impl Into<String>) {
        self.add_entry(LogEntry::new(level, message));
    }

    pub fn debug(&mut self, message: impl Into<String>) {
        self.log(LogLevel::Debug, message);
    }

    pub fn info(&mut self, message: impl Into<String>) {
        self.log(LogLevel::Info, message);
    }

    pub fn warn(&mut self, message: impl Into<String>) {
        self.log(LogLevel::Warn, message);
    }

    pub fn error(&mut self, message: impl Into<String>) {
        self.log(LogLevel::Error, message);
    }

    pub fn scroll_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(1);
    }

    pub fn scroll_down(&mut self) {
        let max_scroll = self.entries.len().saturating_sub(1);
        if self.scroll < max_scroll {
            self.scroll += 1;
        }
    }

    pub fn scroll_to_bottom(&mut self) {
        self.scroll = self.entries.len().saturating_sub(1);
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title("System Log")
            .border_style(Style::default().fg(Color::Gray));

        let inner_area = block.inner(area);
        frame.render_widget(block, area);

        // Calculate visible lines
        let visible_height = inner_area.height as usize;
        let start_idx = self.scroll;
        let end_idx = (start_idx + visible_height).min(self.entries.len());

        let lines: Vec<Line> = self.entries[start_idx..end_idx]
            .iter()
            .map(|entry| {
                let timestamp = entry.timestamp.format("%H:%M:%S");
                Line::from(vec![
                    Span::styled(
                        format!("[{}] ", timestamp),
                        Style::default().fg(Color::DarkGray),
                    ),
                    Span::styled(
                        format!("{:5} ", entry.level.as_str()),
                        Style::default()
                            .fg(entry.level.color())
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(&entry.message),
                ])
            })
            .collect();

        let text = Text::from(lines);
        let paragraph = Paragraph::new(text).wrap(Wrap { trim: false });

        frame.render_widget(paragraph, inner_area);
    }

    pub fn render_compact(&self, frame: &mut Frame, area: Rect, max_lines: usize) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title("Recent Activity")
            .border_style(Style::default().fg(Color::Gray));

        let inner_area = block.inner(area);
        frame.render_widget(block, area);

        let recent_entries: Vec<&LogEntry> = self.entries.iter().rev().take(max_lines).collect();

        let lines: Vec<Line> = recent_entries
            .iter()
            .rev()
            .map(|entry| {
                let symbol = match entry.level {
                    LogLevel::Debug => "•",
                    LogLevel::Info => "ℹ",
                    LogLevel::Warn => "⚠",
                    LogLevel::Error => "✗",
                };

                Line::from(vec![
                    Span::styled(
                        format!("{} ", symbol),
                        Style::default().fg(entry.level.color()),
                    ),
                    Span::raw(&entry.message),
                ])
            })
            .collect();

        let text = Text::from(lines);
        let paragraph = Paragraph::new(text).wrap(Wrap { trim: true });

        frame.render_widget(paragraph, inner_area);
    }
}

impl Default for LogPanel {
    fn default() -> Self {
        Self::new()
    }
}
