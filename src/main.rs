use clap::Parser;
use llm_orchestrator::cli::Cli;
use llm_orchestrator::tui::{App, init_terminal, restore_terminal};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();
    
    // Parse CLI arguments
    let _cli = Cli::parse();
    
    // Initialize the orchestrator
    llm_orchestrator::init().await?;
    
    // Initialize terminal
    let mut terminal = init_terminal()?;
    
    // Run TUI app
    let mut app = App::new();
    let result = app.run(&mut terminal).await;
    
    // Restore terminal regardless of result
    restore_terminal()?;
    
    result
}