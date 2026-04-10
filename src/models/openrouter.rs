use crate::models::types::*;
use anyhow::Result;
use reqwest::Client;

pub struct OpenRouterClient {
    client: Client,
    api_key: String,
    base_url: String,
}

impl OpenRouterClient {
    pub fn new(api_key: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
            base_url: "https://openrouter.ai/api/v1".to_string(),
        }
    }
    
    pub async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        let url = format!("{}/chat/completions", self.base_url);
        
        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?;
        
        if response.status().is_success() {
            let result = response.json().await?;
            Ok(result)
        } else {
            let error_text = response.text().await?;
            Err(anyhow::anyhow!("API error: {}", error_text))
        }
    }
}