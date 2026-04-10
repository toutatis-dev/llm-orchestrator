use crate::core::{ChatMessage, ExecutionPlan};
use crate::executor::progress::ExecutionProgress;
use chrono::{DateTime, Local};
use crossterm::event::{self, Event as CrosstermEvent, KeyCode, KeyEvent, KeyEventKind};
use std::time::Duration;
use tokio::sync::mpsc;

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
    
    pub async fn run(&mut self) -> anyhow::Result<()> {
        // TODO: Initialize terminal and run event loop
        println!("TUI framework initialized");
        println!("App state: {:?}", std::mem::discriminant(&self.state));
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