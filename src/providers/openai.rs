use anyhow::{anyhow, Context, Result};
use bytes::Bytes;
use futures::stream::StreamExt;
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;

use super::{CompletionRequest, CompletionResponse, CompletionStream};

const DEFAULT_ENDPOINT: &str = "https://api.openai.com/v1/chat/completions";

pub struct OpenAiClient {
    http: Client,
    endpoint: String,
    api_key: String,
}

impl OpenAiClient {
    pub fn from_env(
        api_key_override: Option<String>,
        endpoint_override: Option<String>,
        timeout_override: Option<u64>,
    ) -> Result<Self> {
        let api_key = api_key_override
            .or_else(|| std::env::var("OPENAI_API_KEY").ok())
            .ok_or_else(|| anyhow::anyhow!("OPENAI_API_KEY is required. Please set it in ~/.zarz/config.toml or as an environment variable"))?;
        let endpoint = endpoint_override
            .or_else(|| std::env::var("OPENAI_API_URL").ok())
            .unwrap_or_else(|| DEFAULT_ENDPOINT.to_string());

        let timeout_secs = timeout_override
            .or_else(|| {
                std::env::var("OPENAI_TIMEOUT_SECS")
                    .ok()
                    .and_then(|raw| raw.parse::<u64>().ok())
            })
            .unwrap_or(120);

        let http = Client::builder()
            .user_agent("zarz-cli/0.1")
            .timeout(std::time::Duration::from_secs(timeout_secs))
            .build()
            .context("Failed to build HTTP client for OpenAI")?;

        Ok(Self {
            http,
            endpoint,
            api_key,
        })
    }

    pub async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let mut messages = Vec::new();
        if let Some(system) = &request.system_prompt {
            messages.push(json!({
                "role": "system",
                "content": system,
            }));
        }
        messages.push(json!({
            "role": "user",
            "content": request.user_prompt,
        }));

        let payload = json!({
            "model": request.model,
            "max_tokens": request.max_output_tokens,
            "temperature": request.temperature,
            "messages": messages,
        });

        let response = self
            .http
            .post(&self.endpoint)
            .bearer_auth(&self.api_key)
            .json(&payload)
            .send()
            .await
            .context("OpenAI request failed")?;

        let response = response.error_for_status().context("OpenAI returned an error status")?;

        let parsed: OpenAiResponse = response
            .json()
            .await
            .context("Failed to decode OpenAI response")?;

        let text = parsed
            .choices
            .into_iter()
            .find_map(|choice| choice.message.content)
            .ok_or_else(|| anyhow!("OpenAI response did not include content"))?;

        Ok(CompletionResponse { text })
    }

    #[allow(dead_code)]
    pub async fn complete_stream(&self, request: &CompletionRequest) -> Result<CompletionStream> {
        let mut messages = Vec::new();
        if let Some(system) = &request.system_prompt {
            messages.push(json!({
                "role": "system",
                "content": system,
            }));
        }
        messages.push(json!({
            "role": "user",
            "content": request.user_prompt,
        }));

        let payload = json!({
            "model": request.model,
            "max_tokens": request.max_output_tokens,
            "temperature": request.temperature,
            "messages": messages,
            "stream": true,
        });

        let response = self
            .http
            .post(&self.endpoint)
            .bearer_auth(&self.api_key)
            .json(&payload)
            .send()
            .await
            .context("OpenAI streaming request failed")?;

        let response = response
            .error_for_status()
            .context("OpenAI returned an error status")?;

        let stream = response.bytes_stream();
        let text_stream = stream.map(|result| {
            let bytes = result?;
            parse_openai_sse_chunk(&bytes)
        });

        Ok(Box::pin(text_stream))
    }
}

#[allow(dead_code)]
fn parse_openai_sse_chunk(bytes: &Bytes) -> Result<String> {
    let text = String::from_utf8_lossy(bytes);
    let mut result = String::new();

    for line in text.lines() {
        if let Some(data) = line.strip_prefix("data: ") {
            if data == "[DONE]" {
                break;
            }

            if let Ok(chunk) = serde_json::from_str::<StreamChunk>(data) {
                if let Some(choice) = chunk.choices.first() {
                    if let Some(content) = &choice.delta.content {
                        result.push_str(content);
                    }
                }
            }
        }
    }

    Ok(result)
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct StreamChunk {
    choices: Vec<StreamChoice>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct StreamChoice {
    delta: StreamDelta,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct StreamDelta {
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAiResponse {
    choices: Vec<OpenAiChoice>,
}

#[derive(Debug, Deserialize)]
struct OpenAiChoice {
    message: OpenAiMessage,
}

#[derive(Debug, Deserialize)]
struct OpenAiMessage {
    content: Option<String>,
}
