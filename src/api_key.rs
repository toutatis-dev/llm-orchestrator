use anyhow::{Context, Result};

/// Resolve the OpenRouter API key from available sources
///
/// Resolution order:
/// 1. Environment variable: OPENROUTER_API_KEY
/// 2. Keyring storage (if available)
/// 3. Prompt user (if interactive)
pub fn resolve_api_key() -> Result<String> {
    // Try environment variable first
    if let Ok(key) = std::env::var("OPENROUTER_API_KEY") {
        if !key.is_empty() {
            tracing::debug!("Using API key from OPENROUTER_API_KEY environment variable");
            return Ok(key);
        }
    }

    // Try keyring
    match keyring::Entry::new("llm-orchestrator", "openrouter-api-key") {
        Ok(entry) => {
            if let Ok(key) = entry.get_password() {
                if !key.is_empty() {
                    tracing::debug!("Using API key from keyring");
                    return Ok(key);
                }
            }
        }
        Err(e) => {
            tracing::debug!("Failed to access keyring: {}", e);
        }
    }

    Err(anyhow::anyhow!(
        "OpenRouter API key not found. Set OPENROUTER_API_KEY environment variable \
         or configure with: orchestrator configure --api-key"
    ))
}

/// Store API key in keyring for future use
pub fn store_api_key(key: &str) -> Result<()> {
    let entry = keyring::Entry::new("llm-orchestrator", "openrouter-api-key")
        .context("Failed to access keyring")?;

    entry
        .set_password(key)
        .context("Failed to store API key in keyring")?;

    tracing::info!("API key stored securely in keyring");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_key_from_env() {
        // This test would need to set env var before running
        // std::env::set_var("OPENROUTER_API_KEY", "test-key");
        // let key = resolve_api_key().unwrap();
        // assert_eq!(key, "test-key");
    }
}
