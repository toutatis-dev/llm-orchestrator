use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "llm-orchestrator")]
#[command(about = "Interactive LLM orchestrator for code generation")]
pub struct Cli {
    /// Optional task description
    pub task: Option<String>,

    /// Resume a previous session
    #[arg(long)]
    pub resume: Option<String>,

    /// Clean up old failed sessions
    #[arg(long)]
    pub cleanup: bool,

    /// Dry run (don't make changes)
    #[arg(long)]
    pub dry_run: bool,
}
