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
        
        // Main event loop
        while !self.should_quit {
            // Draw UI
            terminal.draw(|f| self.draw(f))?;
            
            // Handle events
            if let Some(event) = event_handler.next().await {
                match event {
                    Event::Key(key) => self.on_key(key),
                    Event::Tick => {}
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
        let mode = match &self.state {
            AppState::Idle => "Idle",
            AppState::Discovery { .. } => "Discovery",
            AppState::Planning { .. } => "Planning",
            AppState::Executing { .. } => "Executing",
            AppState::Paused { .. } => "Paused",
            AppState::Complete { .. } => "Complete",
        };
        layout.render_header(frame, &format!("LLM Orchestrator - Mode: {}", mode));
        
        // Content area
        let (main_area, side_area) = split_content(layout.content, false);
        
        // Main content placeholder
        let content = ratatui::widgets::Paragraph::new("Welcome to LLM Orchestrator\n\nPress 'q' to quit")
            .block(ratatui::widgets::Block::default().borders(ratatui::widgets::Borders::ALL));
        frame.render_widget(content, main_area);
        
        // Input area
        layout.render_input(frame, "", false);
        
        // Footer
        layout.render_footer(frame, "Press 'q' to quit | '?' for help");
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