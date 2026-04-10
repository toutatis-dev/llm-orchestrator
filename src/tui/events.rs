use crossterm::event::{self, KeyEvent, MouseEvent};
use std::time::Duration;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub enum Event {
    /// Terminal tick event
    Tick,
    /// Key press
    Key(KeyEvent),
    /// Mouse event
    Mouse(MouseEvent),
    /// Resize event
    Resize(u16, u16),
    /// Message received from orchestrator
    MessageReceived(String),
    /// Plan generated
    PlanGenerated(crate::core::ExecutionPlan),
    /// Execution update
    ExecutionUpdate(ExecutionUpdate),
    /// File change detected
    FileChanged(FileChangeEvent),
}

#[derive(Debug, Clone)]
pub enum ExecutionUpdate {
    TaskStarted { task_id: String },
    TaskProgress { task_id: String, tokens_generated: usize },
    TaskCompleted { task_id: String },
    TaskFailed { task_id: String, error: String },
    BatchStarted { batch_id: usize },
    BatchCompleted { batch_id: usize },
}

#[derive(Debug, Clone)]
pub enum FileChangeEvent {
    ExternalModification { path: std::path::PathBuf },
    ExpectedModification { path: std::path::PathBuf },
    Deleted(std::path::PathBuf),
    Created(std::path::PathBuf),
}

pub struct EventHandler {
    rx: mpsc::UnboundedReceiver<Event>,
    _tx: mpsc::UnboundedSender<Event>,
}

impl EventHandler {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        
        // Spawn event handling task
        let tx_clone = tx.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(100));
            
            loop {
                interval.tick().await;
                
                // Poll for terminal events
                if crossterm::event::poll(Duration::from_millis(0)).unwrap_or(false) {
                    match crossterm::event::read() {
                        Ok(event::Event::Key(key)) => {
                            let _ = tx_clone.send(Event::Key(key));
                        }
                        Ok(event::Event::Mouse(mouse)) => {
                            let _ = tx_clone.send(Event::Mouse(mouse));
                        }
                        Ok(event::Event::Resize(w, h)) => {
                            let _ = tx_clone.send(Event::Resize(w, h));
                        }
                        _ => {}
                    }
                }
            }
        });
        
        Self { rx, _tx: tx }
    }
    
    pub async fn next(&mut self) -> Option<Event> {
        self.rx.recv().await
    }
}