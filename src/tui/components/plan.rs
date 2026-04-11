use crate::core::{BatchStatus, ExecutionPlan, PlanStatus, WorkerTier};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, Wrap},
    Frame,
};

pub struct PlanPanel {
    scroll: usize,
    selected_batch: Option<usize>,
}

impl PlanPanel {
    pub fn new() -> Self {
        Self {
            scroll: 0,
            selected_batch: None,
        }
    }

    pub fn scroll_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(1);
    }

    pub fn scroll_down(&mut self) {
        self.scroll += 1;
    }

    pub fn select_batch(&mut self, batch_id: usize) {
        self.selected_batch = Some(batch_id);
    }

    pub fn render(&self, frame: &mut Frame, area: Rect, plan: &ExecutionPlan) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(8), // Summary header
                Constraint::Min(10),   // Batches list
                Constraint::Length(5), // Cost estimate
            ])
            .split(area);

        // Render summary
        self.render_summary(frame, chunks[0], plan);

        // Render batches
        self.render_batches(frame, chunks[1], plan);

        // Render cost
        self.render_cost(frame, chunks[2], plan);
    }

    fn render_summary(&self, frame: &mut Frame, area: Rect, plan: &ExecutionPlan) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title("Plan Summary")
            .border_style(Style::default().fg(Color::Cyan));

        let status_color = match plan.status {
            PlanStatus::Draft => Color::Yellow,
            PlanStatus::Approved => Color::Green,
            PlanStatus::InProgress => Color::Blue,
            PlanStatus::Completed => Color::Green,
            PlanStatus::Failed | PlanStatus::ValidationFailed => Color::Red,
            PlanStatus::Cancelled => Color::Gray,
        };

        let text = Text::from(vec![
            Line::from(vec![
                Span::styled("Status: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::styled(
                    format!("{:?}", plan.status),
                    Style::default().fg(status_color),
                ),
            ]),
            Line::from(vec![
                Span::styled("Batches: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(format!("{}", plan.batches.len())),
            ]),
            Line::from(vec![Span::styled(
                "Analysis: ",
                Style::default().add_modifier(Modifier::BOLD),
            )]),
            Line::from(plan.analysis.clone()),
        ]);

        let paragraph = Paragraph::new(text)
            .block(block)
            .wrap(Wrap { trim: true })
            .scroll((self.scroll as u16, 0));

        frame.render_widget(paragraph, area);
    }

    fn render_batches(&self, frame: &mut Frame, area: Rect, plan: &ExecutionPlan) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title("Batches")
            .border_style(Style::default().fg(Color::Cyan));

        // Create table rows for batches
        let header = Row::new(vec!["Batch", "Tier", "Tasks", "Deps", "Status"])
            .style(Style::default().add_modifier(Modifier::BOLD))
            .height(1);

        let rows: Vec<Row> = plan
            .batches
            .iter()
            .map(|batch| {
                let tier_color = match batch.tier {
                    WorkerTier::Simple => Color::Green,
                    WorkerTier::Medium => Color::Yellow,
                    WorkerTier::Complex => Color::Red,
                };

                let status_color = batch.status.map_or(Color::Gray, |s| match s {
                    BatchStatus::Pending => Color::Gray,
                    BatchStatus::InProgress => Color::Blue,
                    BatchStatus::Completed => Color::Green,
                    BatchStatus::Failed => Color::Red,
                    BatchStatus::Skipped => Color::Yellow,
                });

                let is_selected = self.selected_batch == Some(batch.id);
                let row_style = if is_selected {
                    Style::default().bg(Color::DarkGray)
                } else {
                    Style::default()
                };

                Row::new(vec![
                    Cell::from(format!("#{}", batch.id)),
                    Cell::from(format!("{:?}", batch.tier)).style(Style::default().fg(tier_color)),
                    Cell::from(format!("{}", batch.tasks.len())),
                    Cell::from(if batch.dependencies.is_empty() {
                        "-".to_string()
                    } else {
                        format!("{:?}", batch.dependencies)
                    }),
                    Cell::from(format!(
                        "{:?}",
                        batch.status.unwrap_or(BatchStatus::Pending)
                    ))
                    .style(Style::default().fg(status_color)),
                ])
                .style(row_style)
                .height(1)
            })
            .collect();

        let table = Table::new(
            rows,
            &[
                Constraint::Length(8),
                Constraint::Length(10),
                Constraint::Length(8),
                Constraint::Length(12),
                Constraint::Length(12),
            ],
        )
        .header(header)
        .block(block);

        frame.render_widget(table, area);

        // Show selected batch details in popup
        if let Some(selected_id) = self.selected_batch {
            if let Some(batch) = plan.batches.iter().find(|b| b.id == selected_id) {
                self.render_batch_detail_popup(frame, area, batch);
            }
        }
    }

    fn render_batch_detail_popup(
        &self,
        frame: &mut Frame,
        area: Rect,
        batch: &crate::core::TaskBatch,
    ) {
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
            .title(format!("Batch #{} Details", batch.id))
            .border_style(Style::default().fg(Color::Yellow));

        let mut lines = vec![
            Line::from(vec![
                Span::styled("Tier: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(format!("{:?}", batch.tier)),
            ]),
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
        ];

        for task in &batch.tasks {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  • {}: ", task.id),
                    Style::default().fg(Color::Cyan),
                ),
                Span::raw(&task.description),
            ]));
            lines.push(Line::from(vec![
                Span::raw("    Type: "),
                Span::raw(format!("{:?}", task.task_type)),
            ]));
            if !task.expected_outputs.is_empty() {
                lines.push(Line::from(vec![
                    Span::raw("    Outputs: "),
                    Span::raw(
                        task.expected_outputs
                            .iter()
                            .map(|p| p.display().to_string())
                            .collect::<Vec<_>>()
                            .join(", "),
                    ),
                ]));
            }
            lines.push(Line::from(""));
        }

        let paragraph = Paragraph::new(Text::from(lines))
            .block(block)
            .wrap(Wrap { trim: true });

        frame.render_widget(paragraph, popup_area);
    }

    fn render_cost(&self, frame: &mut Frame, area: Rect, plan: &ExecutionPlan) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title("Cost Estimate")
            .border_style(Style::default().fg(Color::Cyan));

        let cost = &plan.total_cost_estimate;

        let text = Text::from(vec![
            Line::from(vec![
                Span::styled(
                    "Estimated Cost: ",
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("${:.4} USD", cost.cost_usd),
                    Style::default().fg(Color::Green),
                ),
            ]),
            Line::from(vec![
                Span::styled("Input Tokens: ", Style::default()),
                Span::raw(format!("{}", cost.input_tokens)),
            ]),
            Line::from(vec![
                Span::styled("Output Tokens: ", Style::default()),
                Span::raw(format!("{}", cost.output_tokens)),
            ]),
            Line::from(vec![
                Span::styled("Total Tokens: ", Style::default()),
                Span::raw(format!("{}", cost.total_tokens())),
            ]),
        ]);

        let paragraph = Paragraph::new(text).block(block);
        frame.render_widget(paragraph, area);
    }
}

impl Default for PlanPanel {
    fn default() -> Self {
        Self::new()
    }
}
