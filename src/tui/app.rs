use crate::core::{ChatMessage, ExecutionPlan, Role};
use crate::executor::progress::ExecutionProgress;
use crate::tui::components::ChatPanel;
use crate::tui::events::{Event, EventHandler};
use chrono::{DateTime, Local};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

pub enum AppState {
    Idle,
    Discovery {
        session_id: String,
        chat: ChatPanel,
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
}

impl Default for App {
    fn default() -> Self {
        let session_id = format!("session-{}", Local::now().format("%Y%m%d-%H%M%S"));
        
        // Start in Discovery mode with welcome message
        let mut chat = ChatPanel::new();
        chat.add_message(ChatMessage::new(
            Role::Orchestrator,
            "Welcome! I'm your orchestrator for code generation tasks.\n\n\
             Describe what you'd like me to help you build, and I'll break it down into \
             manageable tasks for the worker models.",
        ));
        
        Self {
            state: AppState::Discovery { session_id, chat },
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
                    Event::Tick => self.on_tick(),
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
            AppState::Planning { plan, .. } => ("Planning", format!("Plan: {} batches", plan.batches.len())),
            AppState::Executing { progress, .. } => ("Executing", format!("Batch {}/{}", progress.current_batch, progress.total_batches)),
            AppState::Paused { reason, .. } => ("Paused", format!("Paused: {:?}", reason)),
            AppState::Complete { .. } => ("Complete", "Task completed".to_string()),
        };
        layout.render_header(frame, &format!("LLM Orchestrator - {} - {}", mode, status));
        
        // Content area based on state
        let (main_area, side_area) = split_content(layout.content, !matches!(self.state, AppState::Idle));
        
        match &self.state {
            AppState::Discovery { chat, .. } => {
                chat.render(frame, main_area);
            }
            _ => {
                // Placeholder for other states
                let content = ratatui::widgets::Paragraph::new(format!("Mode: {}\n\nPress 'q' to quit", mode))
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
            _ => "Press 'q' to quit | '?' for help",
        };
        layout.render_footer(frame, help_text);
    }
    
    fn on_key(&mut self, key: KeyEvent) {
        if key.kind != KeyEventKind::Press {
            return;
        }
        
        // Global key handlers
        match key.code {
            KeyCode::Char('q') if key.modifiers.is_empty() => {
                self.should_quit = true;
                return;
            }
            _ => {}
        }
        
        // State-specific key handlers
        match &mut self.state {
            AppState::Discovery { chat, .. } => {
                Self::handle_discovery_keys(chat, key);
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
    
    fn handle_discovery_keys(chat: &mut ChatPanel, key: KeyEvent) {
        match key.code {
            // Input handling
            KeyCode::Char(c) => {
                if key.modifiers.contains(KeyModifiers::SHIFT) && c == '\n' {
                    // Shift+Enter for newline
                    chat.input_char('\n');
                } else if key.modifiers.is_empty() {
                    chat.input_char(c);
                }
            }
            KeyCode::Enter => {
                // Send message
                let input = chat.clear_input();
                if !input.trim().is_empty() {
                    chat.add_message(ChatMessage::new(Role::User, &input));
                    // TODO: Send to orchestrator and get response
                    chat.add_message(ChatMessage::new(
                        Role::Orchestrator,
                        "(Orchestrator response will appear here)",
                    ));
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
    
    fn on_tick(&mut self) {
        // Periodic updates (e.g., checking for external file changes)
        self.last_tick = Local::now();
    }
}