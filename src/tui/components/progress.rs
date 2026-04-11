use crate::executor::progress::ExecutionProgress;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols,
    text::{Line, Span, Text},
    widgets::{Block, Borders, Gauge, List, ListItem, Paragraph},
    Frame,
};

pub struct ProgressPanel {
    log_messages: Vec<String>,
}

impl ProgressPanel {
    pub fn new() -> Self {
        Self {
            log_messages: Vec::new(),
        }
    }

    pub fn add_log(&mut self, message: String) {
        let timestamp = chrono::Local::now().format("%H:%M:%S");
        self.log_messages
            .push(format!("[{}] {}", timestamp, message));
        // Keep only last 100 messages
        if self.log_messages.len() > 100 {
            self.log_messages.remove(0);
        }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect, progress: &ExecutionProgress) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(10), Constraint::Min(5)])
            .split(area);

        self.render_progress(frame, chunks[0], progress);
        self.render_log(frame, chunks[1]);
    }

    fn render_progress(&self, frame: &mut Frame, area: Rect, progress: &ExecutionProgress) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title("Execution Progress")
            .border_style(Style::default().fg(Color::Cyan));

        let inner_area = block.inner(area);
        frame.render_widget(block, area);

        // Calculate percentages
        let batch_percent = if progress.total_batches > 0 {
            (progress.current_batch as f64 / progress.total_batches as f64) * 100.0
        } else {
            0.0
        };

        let task_percent = if progress.total_tasks > 0 {
            (progress.tasks_completed as f64 / progress.total_tasks as f64) * 100.0
        } else {
            0.0
        };

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([Constraint::Length(3), Constraint::Length(3)])
            .split(inner_area);

        // Batch progress gauge
        let batch_gauge = Gauge::default()
            .block(Block::default().title("Batches").borders(Borders::ALL))
            .gauge_style(Style::default().fg(Color::Yellow))
            .percent(batch_percent as u16)
            .label(format!(
                "{}/{} ({:.0}%)",
                progress.current_batch, progress.total_batches, batch_percent
            ));
        frame.render_widget(batch_gauge, chunks[0]);

        // Task progress gauge
        let task_gauge = Gauge::default()
            .block(Block::default().title("Tasks").borders(Borders::ALL))
            .gauge_style(Style::default().fg(Color::Green))
            .percent(task_percent as u16)
            .label(format!(
                "{}/{} ({:.0}%)",
                progress.tasks_completed, progress.total_tasks, task_percent
            ));
        frame.render_widget(task_gauge, chunks[1]);
    }

    fn render_log(&self, frame: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title("Execution Log")
            .border_style(Style::default().fg(Color::Gray));

        let items: Vec<ListItem> = self
            .log_messages
            .iter()
            .rev() // Show newest first
            .take(area.height as usize - 2) // Account for borders
            .map(|msg| {
                let style = if msg.contains("ERROR") {
                    Style::default().fg(Color::Red)
                } else if msg.contains("SUCCESS") || msg.contains("Complete") {
                    Style::default().fg(Color::Green)
                } else if msg.contains("WARN") {
                    Style::default().fg(Color::Yellow)
                } else {
                    Style::default().fg(Color::Gray)
                };
                ListItem::new(msg.as_str()).style(style)
            })
            .collect();

        let list = List::new(items).block(block);
        frame.render_widget(list, area);
    }
}

impl Default for ProgressPanel {
    fn default() -> Self {
        Self::new()
    }
}
