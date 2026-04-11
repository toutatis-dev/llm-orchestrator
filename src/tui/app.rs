use crate::config::Config;
use crate::core::{ChatMessage, ExecutionPlan, Role};
use crate::executor::progress::ExecutionProgress;
use crate::tui::components::ChatPanel;
use crate::tui::events::{Event, EventHandler};
use chrono::{DateTime, Local};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use tokio::sync::mpsc;

pub enum AppState {
    Idle,
    Discovery {
        session_id: String,
        chat: ChatPanel,
    },
    GeneratingPlan {
        session_id: String,
        chat: ChatPanel,
        task_description: String,
    },
    Planning {
        session_id: String,
        plan: ExecutionPlan,
        approval_mode: ApprovalMode,
    },
    Executing {
        session_id: String,
        progress: ExecutionProgress,
    },
    Paused {
        session_id: String,
        reason: PauseReason,
    },
    Complete {
        session_id: String,
    },
    Error {
        session_id: String,
        message: String,
        previous_state: Box<AppState>,
    },
}

pub enum ApprovalMode {
    WholePlan,
    Granular { current_batch: usize },
}

#[derive(Debug)]
pub enum PauseReason {
    ExternalFileChange { path: std::path::PathBuf },
    TaskFailedAfterRetry { task_id: String, error: String },
    UserRequest,
}

pub struct App {
    pub state: AppState,
    pub should_quit: bool,
    pub last_tick: DateTime<Local>,
    pub config: Config,
    event_tx: mpsc::UnboundedSender<Event>,
}

impl App {
    pub fn new() -> anyhow::Result<Self> {
        let session_id = format!("session-{}", Local::now().format("%Y%m%d-%H%M%S"));
        let config = Config::load()?;

        // Create event channel for async operations
        let (event_tx, _event_rx) = mpsc::unbounded_channel();

        // Start in Discovery mode with welcome message
        let mut chat = ChatPanel::new();
        chat.add_message(ChatMessage::new(
            Role::Orchestrator,
            "Welcome! I'm your orchestrator for code generation tasks.\n\n\
             Describe what you'd like me to help you build, and I'll break it down into \
             manageable tasks for the worker models.",
        ));

        Ok(Self {
            state: AppState::Discovery { session_id, chat },
            should_quit: false,
            last_tick: Local::now(),
            config,
            event_tx,
        })
    }

    pub async fn run(&mut self, terminal: &mut super::Tui) -> anyhow::Result<()> {
        let mut event_handler = EventHandler::new();

        // Main event loop
        while !self.should_quit {
            // Draw UI
            terminal.draw(|f| self.draw(f))?;

            // Handle events
            if let Some(event) = event_handler.next().await {
                match event {
                    Event::Key(key) => self.on_key(key).await,
                    Event::Tick => self.on_tick(),
                    Event::PlanGenerated(plan) => self.on_plan_generated(plan),
                    Event::MessageReceived(msg) => self.on_message_received(msg),
                    _ => {}
                }
            }
        }

        Ok(())
    }

    fn draw(&self, frame: &mut ratatui::Frame) {
        use super::layout::{split_content, MainLayout};

        let layout = MainLayout::new(frame);

        // Header
        let (mode, status) = match &self.state {
            AppState::Idle => ("Idle", "".to_string()),
            AppState::Discovery { .. } => ("Discovery", "Chat with the orchestrator".to_string()),
            AppState::GeneratingPlan { .. } => ("Planning", "Generating plan...".to_string()),
            AppState::Planning { plan, .. } => (
                "Planning",
                format!("Plan: {} batches", plan.batches.len()),
            ),
            AppState::Executing { progress, .. } => (
                "Executing",
                format!("Batch {}/{}", progress.current_batch, progress.total_batches),
            ),
            AppState::Paused { reason, .. } => ("Paused", format!("Paused: {:?}", reason)),
            AppState::Complete { .. } => ("Complete", "Task completed".to_string()),
            AppState::Error { message, .. } => ("Error", message.clone()),
        };
        layout.render_header(frame, &format!("LLM Orchestrator - {} - {}", mode, status));

        // Content area based on state
        let (main_area, side_area) = split_content(layout.content, !matches!(self.state, AppState::Idle));

        match &self.state {
            AppState::Discovery { chat, .. } | AppState::GeneratingPlan { chat, .. } => {
                chat.render(frame, main_area);
            }
            _ => {
                // Placeholder for other states
                let content = ratatui::widgets::Paragraph::new(format!(
                    "Mode: {}\n\nPress 'q' to quit",
                    mode
                ))
                .block(ratatui::widgets::Block::default().borders(ratatui::widgets::Borders::ALL));
                frame.render_widget(content, main_area);
            }
        }

        // Side panel (when applicable)
        if !matches!(side_area, ratatui::layout::Rect { width: 0, height: 0, .. }) {
            let side = ratatui::widgets::Paragraph::new("Side panel\n(Cost tracking, etc.)")
                .block(ratatui::widgets::Block::default().borders(ratatui::widgets::Borders::ALL));
            frame.render_widget(side, side_area);
        }

        // Footer with context-sensitive help
        let help_text = match &self.state {
            AppState::Discovery { .. } => "Enter: Send | Shift+Enter: Newline | ↑/↓: Scroll | q: Quit",
            AppState::GeneratingPlan { .. } => "Generating plan, please wait...",
            AppState::Error { .. } => "Press Enter to dismiss error | q: Quit",
            _ => "Press 'q' to quit | '?' for help",
        };
        layout.render_footer(frame, help_text);
    }

    async fn on_key(&mut self, key: KeyEvent) {
        if key.kind != KeyEventKind::Press {
            return;
        }

        // Global key handlers
        match key.code {
            KeyCode::Char('q') if key.modifiers.is_empty() => {
                self.should_quit = true;
                return;
            }
            KeyCode::Enter if matches!(self.state, AppState::Error { .. }) => {
                // Dismiss error and return to previous state
                if let AppState::Error { previous_state, .. } = std::mem::replace(&mut self.state, AppState::Idle) {
                    self.state = *previous_state;
                }
                return;
            }
            _ => {}
        }

        // State-specific key handlers
        match &mut self.state {
            AppState::Discovery { chat, session_id } => {
                match key.code {
                    // Input handling
                    KeyCode::Char(c) => {
                        if key.modifiers.contains(KeyModifiers::SHIFT) && c == '\n' {
                            chat.input_char('\n');
                        } else if key.modifiers.is_empty() {
                            chat.input_char(c);
                        }
                    }
                    KeyCode::Enter => {
                        let input = chat.clear_input();
                        if !input.trim().is_empty() {
                            let task_description = input.clone();
                            chat.add_message(ChatMessage::new(Role::User, &input));
                            chat.add_message(ChatMessage::new(
                                Role::Orchestrator,
                                "Analyzing your request and generating a plan...",
                            ));

                            // Clone what we need before state transition
                            let session_id = session_id.clone();
                            let chat_clone = chat.clone();
                            let event_tx = self.event_tx.clone();
                            let config = self.config.clone();

                            // Transition to GeneratingPlan state
                            self.state = AppState::GeneratingPlan {
                                session_id: session_id.clone(),
                                chat: chat_clone,
                                task_description: task_description.clone(),
                            };

                            // Spawn plan generation task
                            tokio::spawn(async move {
                                match crate::create_planner(&config) {
                                    Ok(planner) => {
                                        match planner.generate_plan(&task_description, None).await {
                                            Ok(plan) => {
                                                let _ = event_tx.send(Event::PlanGenerated(plan));
                                            }
                                            Err(e) => {
                                                let _ = event_tx.send(Event::MessageReceived(format!(
                                                    "Failed to generate plan: {}",
                                                    e
                                                )));
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        let _ = event_tx.send(Event::MessageReceived(format!(
                                            "Failed to initialize planner: {}",
                                            e
                                        )));
                                    }
                                }
                            });
                        }
                    }
                    KeyCode::Backspace => chat.input_backspace(),
                    KeyCode::Delete => chat.input_delete(),
                    KeyCode::Left => chat.input_left(),
                    KeyCode::Right => chat.input_right(),
                    KeyCode::Up => chat.scroll_up(),
                    KeyCode::Down => chat.scroll_down(),
                    _ => {}
                }
            }
            AppState::GeneratingPlan { .. } => {
                // Block input while generating
            }
            _ => {
                // Other states - basic navigation
                match key.code {
                    KeyCode::Char('q') => self.should_quit = true,
                    _ => {}
                }
            }
        }
    }

    fn on_plan_generated(&mut self, plan: ExecutionPlan) {
        match &mut self.state {
            AppState::GeneratingPlan { session_id, chat, .. } => {
                chat.add_message(ChatMessage::new(
                    Role::Orchestrator,
                    format!(
                        "Plan generated successfully!\n\n{}",
                        plan.analysis
                    ),
                ));

                // Transition to Planning state
                let session_id = session_id.clone();
                let _chat = chat.clone();
                self.state = AppState::Planning {
                    session_id,
                    plan,
                    approval_mode: ApprovalMode::WholePlan,
                };
            }
            _ => {
                // Unexpected plan in wrong state, just log it
                tracing::warn!("Received plan in unexpected state");
            }
        }
    }

    fn on_message_received(&mut self, msg: String) {
        // Show error state
        let session_id = match &self.state {
            AppState::GeneratingPlan { session_id, .. } => session_id.clone(),
            AppState::Discovery { session_id, .. } => session_id.clone(),
            _ => return,
        };

        let previous_state = Box::new(std::mem::replace(&mut self.state, AppState::Idle));
        self.state = AppState::Error {
            session_id,
            message: msg,
            previous_state,
        };
    }

    fn on_tick(&mut self) {
        // Periodic updates (e.g., checking for external file changes)
        self.last_tick = Local::now();
    }
}
