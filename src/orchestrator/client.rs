use crate::core::ChatMessage;
use crate::models::openrouter::OpenRouterClient;
use crate::models::types::*;
use anyhow::Result;
use futures::StreamExt;
use eventsource_stream::Eventsource;

pub struct OrchestratorClient {
    client: OpenRouterClient,
    model: String,
    temperature: f32,
}

pub struct StreamChunk {
    pub content: String,
    pub is_finished: bool,
}

impl OrchestratorClient {
    pub fn new(api_key: String, model: impl Into<String>, temperature: f32) -> Self {
        Self {
            client: OpenRouterClient::new(api_key),
            model: model.into(),
            temperature,
        }
    }
    
    /// Non-streaming chat - returns full response
    pub async fn chat(&self, messages: &[ChatMessage]) -> Result<(String, TokenUsage)> {
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
            stream: false,
        };
        
        let response = self.client.complete(request).await?;
        let content = response
            .choices
            .first()
            .map(|c| c.message.content.clone())
            .unwrap_or_default();
        
        let usage = TokenUsage {
            input: response.usage.prompt_tokens,
            output: response.usage.completion_tokens,
        };
        
        Ok((content, usage))
    }
    
    /// Streaming chat - calls callback with each chunk
    pub async fn chat_streaming<F>(
        &self,
        messages: &[ChatMessage],
        mut on_chunk: F,
    ) -> Result<TokenUsage>
    where
        F: FnMut(&str),
    {
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
        
        let response = self.client.complete_streaming(request).await?;
        let mut stream = response.bytes_stream();
        
        let mut buffer = String::new();
        
        while let Some(chunk) = stream.next().await {
            match chunk {
                Ok(bytes) => {
                    if let Ok(text) = std::str::from_utf8(&bytes) {
                        // Parse SSE format
                        for line in text.lines() {
                            if line.starts_with("data: ") {
                                let data = &line[6..];
                                if data == "[DONE]" {
                                    break;
                                }
                                
                                if let Ok(chunk) = serde_json::from_str::<StreamResponse>(data) {
                                    if let Some(delta) = chunk.choices.first().and_then(|c| c.delta.content.as_ref()) {
                                        buffer.push_str(delta);
                                        on_chunk(delta);
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("Stream error: {}", e);
                    break;
                }
            }
        }
        
        // Note: Streaming doesn't return usage, so we estimate
        Ok(TokenUsage {
            input: 0, // Unknown for streaming
            output: 0, // Unknown for streaming
        })
    }
}

use crate::core::TokenCount as TokenUsage;

#[derive(Debug, serde::Deserialize)]
struct StreamResponse {
    choices: Vec<StreamChoice>,
}

#[derive(Debug, serde::Deserialize)]
struct StreamChoice {
    delta: StreamDelta,
    finish_reason: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct StreamDelta {
    content: Option<String>,
}