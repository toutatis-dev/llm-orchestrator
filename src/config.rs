use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub general: GeneralConfig,
    pub orchestrator: OrchestratorConfig,
    pub interactive: InteractiveConfig,
    pub tiers: HashMap<String, TierConfig>,
    pub context: ContextConfig,
    pub file_watcher: FileWatcherConfig,
    pub git: GitConfig,
    pub tui: TuiConfig,
    pub rate_limit: RateLimitConfig,
}

impl Default for Config {
    fn default() -> Self {
        let mut tiers = HashMap::new();

        tiers.insert(
            "simple".to_string(),
            TierConfig {
                model: "qwen/qwen3.5-4b".to_string(),
                provider: "openrouter".to_string(),
                context_window: 32768,
                max_tokens: 4096,
                cost_per_1k_input: rust_decimal::Decimal::from_str_exact("0.0001").unwrap(),
                cost_per_1k_output: rust_decimal::Decimal::from_str_exact("0.0002").unwrap(),
            },
        );

        tiers.insert(
            "medium".to_string(),
            TierConfig {
                model: "qwen/qwen3.5-9b".to_string(),
                provider: "openrouter".to_string(),
                context_window: 65536,
                max_tokens: 8192,
                cost_per_1k_input: rust_decimal::Decimal::from_str_exact("0.0002").unwrap(),
                cost_per_1k_output: rust_decimal::Decimal::from_str_exact("0.0004").unwrap(),
            },
        );

        tiers.insert(
            "complex".to_string(),
            TierConfig {
                model: "qwen/qwen3.5-32b".to_string(),
                provider: "openrouter".to_string(),
                context_window: 65536,
                max_tokens: 8192,
                cost_per_1k_input: rust_decimal::Decimal::from_str_exact("0.0006").unwrap(),
                cost_per_1k_output: rust_decimal::Decimal::from_str_exact("0.0012").unwrap(),
            },
        );

        Self {
            general: GeneralConfig::default(),
            orchestrator: OrchestratorConfig::default(),
            interactive: InteractiveConfig::default(),
            tiers,
            context: ContextConfig::default(),
            file_watcher: FileWatcherConfig::default(),
            git: GitConfig::default(),
            tui: TuiConfig::default(),
            rate_limit: RateLimitConfig::default(),
        }
    }
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        let config_path = Path::new(".orchestrator").join("config.toml");

        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)?;
            let config: Config = toml::from_str(&content)?;
            Ok(config)
        } else {
            let config = Config::default();
            config.save()?;
            Ok(config)
        }
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let config_path = Path::new(".orchestrator").join("config.toml");
        std::fs::create_dir_all(".orchestrator")?;
        let content = toml::to_string_pretty(self)?;
        std::fs::write(config_path, content)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    pub execution_mode: String,
    pub max_concurrent_workers: usize,
    pub auto_retry: bool,
    pub max_retries: usize,
    pub escalate_on_retry: bool,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            execution_mode: "in_process".to_string(),
            max_concurrent_workers: 5,
            auto_retry: true,
            max_retries: 1,
            escalate_on_retry: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestratorConfig {
    pub provider: String,
    pub model: String,
    pub temperature: f32,
    pub max_context: usize,
    pub stream: bool,
    pub stream_buffer_lines: usize,
}

impl Default for OrchestratorConfig {
    fn default() -> Self {
        Self {
            provider: "openrouter".to_string(),
            model: "moonshotai/kimi-k2.5".to_string(),
            temperature: 0.1,
            max_context: 200000,
            stream: true,
            stream_buffer_lines: 3,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractiveConfig {
    pub auto_plan: bool,
    pub cost_warnings: bool,
    pub warning_threshold: f64,
    pub show_token_estimates: bool,
    pub multiline_input: bool,
}

impl Default for InteractiveConfig {
    fn default() -> Self {
        Self {
            auto_plan: false,
            cost_warnings: true,
            warning_threshold: 1.0,
            show_token_estimates: true,
            multiline_input: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TierConfig {
    pub model: String,
    pub provider: String,
    pub context_window: usize,
    pub max_tokens: usize,
    pub cost_per_1k_input: rust_decimal::Decimal,
    pub cost_per_1k_output: rust_decimal::Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextConfig {
    pub mode: String,
    pub max_files: usize,
    pub max_tokens: usize,
}

impl Default for ContextConfig {
    fn default() -> Self {
        Self {
            mode: "simple".to_string(),
            max_files: 50,
            max_tokens: 100000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileWatcherConfig {
    pub enabled: bool,
    pub debounce_ms: u64,
    pub notify_on_external_change: bool,
}

impl Default for FileWatcherConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            debounce_ms: 500,
            notify_on_external_change: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitConfig {
    pub auto_branch: bool,
    pub branch_prefix: String,
    pub auto_commit: bool,
    pub commit_message_template: String,
    pub cleanup_on_success: bool,
}

impl Default for GitConfig {
    fn default() -> Self {
        Self {
            auto_branch: true,
            branch_prefix: "orchestrator/".to_string(),
            auto_commit: false,
            commit_message_template: "[orchestrator] {task_summary}".to_string(),
            cleanup_on_success: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuiConfig {
    pub refresh_rate_ms: u64,
    pub theme: String,
}

impl Default for TuiConfig {
    fn default() -> Self {
        Self {
            refresh_rate_ms: 100,
            theme: "default".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    pub max_retries: usize,
    pub initial_backoff_ms: u64,
    pub max_backoff_ms: u64,
    pub multiplier: f64,
    pub jitter: f64,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_backoff_ms: 1000,
            max_backoff_ms: 60000,
            multiplier: 2.0,
            jitter: 0.25,
        }
    }
}
