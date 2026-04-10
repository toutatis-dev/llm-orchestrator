use clap::Parser;
use llm_orchestrator::cli::Cli;
use llm_orchestrator::tui::App;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();
    
    // Parse CLI arguments
    let cli = Cli::parse();
    
    // Initialize the orchestrator
    llm_orchestrator::init().await?;
    
    // Run TUI app
    let mut app = App::new();
    app.run().await?;
    
    Ok(())
}