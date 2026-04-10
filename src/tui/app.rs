use crate::core::{ChatMessage, ExecutionPlan};
use crate::executor::progress::ExecutionProgress;
use crate::tui::events::{Event, EventHandler};
use chrono::{DateTime, Local};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};

pub enum AppState {
    Idle,
    Discovery {
        session_id: String,
        messages: Vec<ChatMessage>,
        input_buffer: String,
        scroll: usize,
    },
    Planning {
        session_id: String,
        messages: Vec<ChatMessage>,
        plan: ExecutionPlan,
        approval_mode: ApprovalMode,
        chat_input: String,
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
}

pub enum ApprovalMode {
    WholePlan,
    Granular { current_batch: usize },
}

pub enum PauseReason {
    ExternalFileChange { path: std::path::PathBuf },
    TaskFailedAfterRetry { task_id: String, error: String },
    UserRequest,
}

pub struct App {
    pub state: AppState,
    pub should_quit: bool,
    pub last_tick: DateTime<Local>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            state: AppState::Idle,
            should_quit: false,
            last_tick: Local::now(),
        }
    }
}

impl App {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub async fn run(&mut self, terminal: &mut super::Tui) -> anyhow::Result<()> {
        let mut event_handler = EventHandler::new();
        
        // Initial draw
        terminal.draw(|f| {
            let size = f.area();
            let text = ratatui::widgets::Paragraph::new("LLM Orchestrator - Press 'q' to quit");
            f.render_widget(text, size);
        })?;
        
        // Main event loop
        while !self.should_quit {
            // Handle events
            if let Some(event) = event_handler.next().await {
                match event {
                    Event::Key(key) => self.on_key(key),
                    Event::Tick => {}
                    _ => {}
                }
            }
            
            // Redraw
            terminal.draw(|f| {
                let size = f.area();
                let text = ratatui::widgets::Paragraph::new(format!(
                    "LLM Orchestrator\nState: {:?}\nPress 'q' to quit",
                    std::mem::discriminant(&self.state)
                ));
                f.render_widget(text, size);
            })?;
        }
        
        Ok(())
    }
    
    pub fn on_key(&mut self, key: KeyEvent) {
        if key.kind != KeyEventKind::Press {
            return;
        }
        
        match key.code {
            KeyCode::Char('q') => {
                self.should_quit = true;
            }
            _ => {}
        }
    }
}