use crate::core::{ExecutionPlan, TaskBatch, WorkerTier};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols,
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Frame,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WizardAction {
    ApproveBatch,
    RejectBatch,
    ApproveAll,
    NextBatch,
    PreviousBatch,
    EditBatch,
    Cancel,
    Execute,
}

pub struct WizardState {
    /// Current batch being reviewed (index into plan.batches)
    current_batch_index: usize,
    /// Which batches have been approved
    approved_batches: Vec<bool>,
    /// Whether to show batch detail modal
    show_detail: bool,
}

impl WizardState {
    pub fn new(plan: &ExecutionPlan) -> Self {
        Self {
            current_batch_index: 0,
            approved_batches: vec![false; plan.batches.len()],
            show_detail: false,
        }
    }

    pub fn current_batch<'a>(&self, plan: &'a ExecutionPlan) -> Option<&'a TaskBatch> {
        plan.batches.get(self.current_batch_index)
    }

    pub fn is_current_batch_approved(&self) -> bool {
        self.approved_batches
            .get(self.current_batch_index)
            .copied()
            .unwrap_or(false)
    }

    pub fn approve_current(&mut self) {
        if let Some(approved) = self.approved_batches.get_mut(self.current_batch_index) {
            *approved = true;
        }
    }

    pub fn reject_current(&mut self) {
        if let Some(approved) = self.approved_batches.get_mut(self.current_batch_index) {
            *approved = false;
        }
    }

    pub fn approve_all(&mut self) {
        for approved in &mut self.approved_batches {
            *approved = true;
        }
    }

    pub fn next_batch(&mut self, plan: &ExecutionPlan) {
        if self.current_batch_index < plan.batches.len().saturating_sub(1) {
            self.current_batch_index += 1;
        }
    }

    pub fn previous_batch(&mut self) {
        if self.current_batch_index > 0 {
            self.current_batch_index -= 1;
        }
    }

    pub fn all_approved(&self) -> bool {
        self.approved_batches.iter().all(|&a| a)
    }

    pub fn approved_count(&self) -> usize {
        self.approved_batches.iter().filter(|&&a| a).count()
    }

    pub fn toggle_detail(&mut self) {
        self.show_detail = !self.show_detail;
    }

    pub fn is_complete(&self) -> bool {
        self.all_approved()
    }
}

pub struct WizardPanel;

impl WizardPanel {
    pub fn new() -> Self {
        Self
    }

    pub fn render(&self, frame: &mut Frame, area: Rect, plan: &ExecutionPlan, state: &WizardState) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Progress header
                Constraint::Min(10),   // Current batch details
                Constraint::Length(8), // Task list
                Constraint::Length(3), // Help/actions
            ])
            .split(area);

        // Render progress
        self.render_progress(frame, chunks[0], plan, state);

        // Render current batch
        if let Some(batch) = state.current_batch(plan) {
            self.render_batch_header(frame, chunks[1], batch, state);
            self.render_task_list(frame, chunks[2], batch);
        }

        // Render actions
        self.render_actions(frame, chunks[3], state);

        // Render detail modal if requested
        if state.show_detail {
            if let Some(batch) = state.current_batch(plan) {
                self.render_batch_detail_modal(frame, area, batch);
            }
        }
    }

    fn render_progress(
        &self,
        frame: &mut Frame,
        area: Rect,
        plan: &ExecutionPlan,
        state: &WizardState,
    ) {
        let approved = state.approved_count();
        let total = plan.batches.len();
        let progress = if total > 0 {
            (approved as f32 / total as f32) * 100.0
        } else {
            0.0
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .title("Approval Progress")
            .border_style(Style::default().fg(Color::Cyan));

        // Create progress bar
        let bar_width = area.width.saturating_sub(4) as usize;
        let filled = ((progress / 100.0) * bar_width as f32) as usize;
        let bar = format!(
            "[{}{}] {:.0}%",
            "█".repeat(filled),
            "░".repeat(bar_width.saturating_sub(filled)),
            progress
        );

        let color = if approved == total {
            Color::Green
        } else {
            Color::Yellow
        };

        let text = Text::from(vec![
            Line::from(vec![
                Span::styled(
                    format!("Batch {} of {} ", state.current_batch_index + 1, total),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("({} approved)", approved),
                    Style::default().fg(color),
                ),
            ]),
            Line::from(Span::styled(bar, Style::default().fg(color))),
        ]);

        let paragraph = Paragraph::new(text).block(block);
        frame.render_widget(paragraph, area);
    }

    fn render_batch_header(
        &self,
        frame: &mut Frame,
        area: Rect,
        batch: &TaskBatch,
        state: &WizardState,
    ) {
        let approved = state.is_current_batch_approved();
        let status_color = if approved {
            Color::Green
        } else {
            Color::Yellow
        };
        let status_text = if approved {
            "✓ APPROVED"
        } else {
            "⏳ PENDING"
        };

        let tier_color = match batch.tier {
            WorkerTier::Simple => Color::Green,
            WorkerTier::Medium => Color::Yellow,
            WorkerTier::Complex => Color::Red,
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .title(format!("Batch #{} Review", batch.id))
            .border_style(Style::default().fg(status_color));

        let mut lines = vec![
            Line::from(vec![
                Span::styled("Status: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::styled(
                    status_text,
                    Style::default()
                        .fg(status_color)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::styled("Tier: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::styled(format!("{:?}", batch.tier), Style::default().fg(tier_color)),
            ]),
            Line::from(vec![
                Span::styled("Tasks: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(format!("{}", batch.tasks.len())),
            ]),
        ];

        if !batch.dependencies.is_empty() {
            lines.push(Line::from(vec![
                Span::styled(
                    "Dependencies: ",
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::raw(format!("Batch(es) {:?}", batch.dependencies)),
            ]));
        }

        let paragraph = Paragraph::new(Text::from(lines))
            .block(block)
            .wrap(Wrap { trim: true });

        frame.render_widget(paragraph, area);
    }

    fn render_task_list(&self, frame: &mut Frame, area: Rect, batch: &TaskBatch) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title("Tasks in this Batch")
            .border_style(Style::default().fg(Color::Blue));

        let items: Vec<ListItem> = batch
            .tasks
            .iter()
            .map(|task| {
                let lines = vec![
                    Line::from(vec![
                        Span::styled(format!("• {}: ", task.id), Style::default().fg(Color::Cyan)),
                        Span::raw(&task.description),
                    ]),
                    Line::from(vec![
                        Span::raw("  Type: "),
                        Span::styled(
                            format!("{:?}", task.task_type),
                            Style::default().fg(Color::Gray),
                        ),
                    ]),
                ];
                ListItem::new(Text::from(lines))
            })
            .collect();

        let list = List::new(items).block(block);
        frame.render_widget(list, area);
    }

    fn render_actions(&self, frame: &mut Frame, area: Rect, state: &WizardState) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title("Actions")
            .border_style(Style::default().fg(Color::Cyan));

        let help_text = if state.all_approved() {
            "Enter: Execute Plan | r: Reject Current | p/n: Previous/Next | d: Details | a: Approve All | q: Quit"
        } else {
            "Enter: Approve Current | r: Reject Current | p/n: Previous/Next | d: Details | a: Approve All | q: Quit"
        };

        let paragraph = Paragraph::new(help_text)
            .block(block)
            .alignment(Alignment::Center);

        frame.render_widget(paragraph, area);
    }

    fn render_batch_detail_modal(&self, frame: &mut Frame, area: Rect, batch: &TaskBatch) {
        // Calculate popup area (centered, 80% of available space)
        let popup_width = (area.width as f32 * 0.8) as u16;
        let popup_height = (area.height as f32 * 0.8) as u16;
        let popup_x = area.x + (area.width.saturating_sub(popup_width)) / 2;
        let popup_y = area.y + (area.height.saturating_sub(popup_height)) / 2;
        let popup_area = Rect::new(popup_x, popup_y, popup_width, popup_height);

        // Clear background
        frame.render_widget(Clear, popup_area);

        let block = Block::default()
            .borders(Borders::ALL)
            .title(format!("Batch #{} - Full Details", batch.id))
            .border_style(Style::default().fg(Color::Yellow));

        let mut lines = vec![
            Line::from(Span::styled(
                format!("Tier: {:?}", batch.tier),
                Style::default().add_modifier(Modifier::BOLD),
            )),
            Line::from(vec![
                Span::styled(
                    "Dependencies: ",
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::raw(if batch.dependencies.is_empty() {
                    "None".to_string()
                } else {
                    format!("{:?}", batch.dependencies)
                }),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "Tasks:",
                Style::default().add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
        ];

        for task in &batch.tasks {
            lines.push(Line::from(vec![Span::styled(
                format!("Task {}: ", task.id),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )]));
            lines.push(Line::from(vec![
                Span::raw("  Description: "),
                Span::raw(&task.description),
            ]));
            lines.push(Line::from(vec![
                Span::raw("  Type: "),
                Span::styled(
                    format!("{:?}", task.task_type),
                    Style::default().fg(Color::Gray),
                ),
            ]));
            if !task.inputs.is_empty() {
                lines.push(Line::from(vec![
                    Span::raw("  Inputs: "),
                    Span::raw(
                        task.inputs
                            .iter()
                            .map(|p| p.display().to_string())
                            .collect::<Vec<_>>()
                            .join(", "),
                    ),
                ]));
            }
            if !task.expected_outputs.is_empty() {
                lines.push(Line::from(vec![
                    Span::raw("  Outputs: "),
                    Span::raw(
                        task.expected_outputs
                            .iter()
                            .map(|p| p.display().to_string())
                            .collect::<Vec<_>>()
                            .join(", "),
                    ),
                ]));
            }
            if !task.context.is_empty() {
                lines.push(Line::from(vec![
                    Span::raw("  Context: "),
                    Span::raw(&task.context),
                ]));
            }
            lines.push(Line::from(""));
        }

        let paragraph = Paragraph::new(Text::from(lines))
            .block(block)
            .wrap(Wrap { trim: true });

        frame.render_widget(paragraph, popup_area);
    }
}

impl Default for WizardPanel {
    fn default() -> Self {
        Self::new()
    }
}
