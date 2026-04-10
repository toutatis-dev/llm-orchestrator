use crate::core::ChatMessage;
use crate::models::openrouter::OpenRouterClient;
use crate::models::types::*;
use anyhow::Result;

pub struct OrchestratorClient {
    client: OpenRouterClient,
    model: String,
    temperature: f32,
}

impl OrchestratorClient {
    pub fn new(api_key: String, model: impl Into<String>, temperature: f32) -> Self {
        Self {
            client: OpenRouterClient::new(api_key),
            model: model.into(),
            temperature,
        }
    }
    
    pub async fn chat(&self, messages: &[ChatMessage]) -> Result<String> {
        let request = CompletionRequest {
            model: self.model.clone(),
            messages: messages
                .iter()
                .map(|m| Message {
                    role: m.role.to_string().to_lowercase(),
                    content: m.content.clone(),
                })
                .collect(),
            temperature: self.temperature,
            max_tokens: 4096,
            stream: true,
        };
        
        // TODO: Implement streaming
        let response = self.client.complete(request).await?;
        
        Ok(response
            .choices
            .first()
            .map(|c| c.message.content.clone())
            .unwrap_or_default())
    }
}